use super::*;
use crate::import_cxf_passkey_json;
use openssl::{ec::EcKey, pkey::PKey, rsa::Rsa};
use serde_json::{json, Value};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .unwrap()
}

fn encode_unchecked(value: &impl Serialize) -> String {
    let mut bytes = Vec::new();
    ciborium::into_writer(value, &mut bytes).unwrap();
    URL_SAFE_NO_PAD.encode(bytes)
}

#[test]
fn android_fixtures_roundtrip_all_algorithms_and_metadata() {
    for (name, alg, length) in [("es256", -7, 32), ("ed25519", -8, 32), ("rs256", -257, 512)] {
        let input = fixture(&format!("{name}.b64"));
        let original = decode_passkey_storage_value(input.trim()).unwrap();
        assert_eq!(original.alg, alg);
        assert_eq!(original.private_key.len(), length);
        assert_eq!(original.sign_count, 7);
        assert_eq!(original.created, 1700000000);
        assert_eq!(original.zone, "Europe/Amsterdam");
        assert!(original.user.reveal_name);
        assert_eq!(original.user.id, [0, 127, 128, 255]);
        assert_eq!(
            decode_passkey_storage_value(&encode_passkey_storage_value(&original).unwrap())
                .unwrap(),
            original
        );
        assert!(inspect_passkey_storage_value(input.trim()).unwrap().is_ok());
        let cxf = import_cxf_passkey_json(&fixture(&format!("{name}.cxf.json"))).unwrap();
        assert_eq!(cxf.private_key, original.private_key);
        assert_eq!(cxf.id, original.id);
        assert_eq!(cxf.alg, original.alg);
        assert_eq!(cxf.sign_count, 0);
        assert_eq!(cxf.zone, "UTC");
        assert!(!cxf.user.reveal_name);
    }
}

#[test]
fn byte_fields_encode_as_integer_arrays_and_paths_use_full_id() {
    let credential = decode_passkey_storage_value(fixture("es256.b64").trim()).unwrap();
    let entry = build_passkey_storage_entry(&credential).unwrap();
    assert_eq!(
        entry.label,
        format!(
            "passkeys/example.com/{}",
            (0u8..32).map(|b| format!("{b:02x}")).collect::<String>()
        )
    );
    validate_passkey_entry_label(&credential, &entry.label).unwrap();
    assert!(validate_passkey_entry_label(&credential, "passkeys/example.com/alice").is_err());
    assert!(validate_passkey_entry_label(
        &credential,
        &entry.label.replace("example.com", "wrong.com")
    )
    .is_err());
    let decoded = URL_SAFE_NO_PAD.decode(&entry.contents).unwrap();
    let value: ciborium::Value = ciborium::from_reader(decoded.as_slice()).unwrap();
    let map = value.as_map().unwrap();
    for name in ["id", "private_key"] {
        let field = &map
            .iter()
            .find(|(key, _)| key.as_text() == Some(name))
            .unwrap()
            .1;
        assert!(field
            .as_array()
            .unwrap()
            .iter()
            .all(|item| item.as_integer().is_some()));
    }
    assert!(!format!("{entry:?}").contains(&entry.contents));
    assert!(!format!("{credential:?}").contains(&format!("{:?}", credential.private_key)));
}

#[test]
fn malformed_records_fail_without_becoming_passwords() {
    let original = decode_passkey_storage_value(fixture("es256.b64").trim()).unwrap();
    let value = serde_json::to_value(&original).unwrap();
    for (key, replacement) in [
        ("alg", json!(-999)),
        ("id", json!([1])),
        ("private_key", json!([256])),
        ("private_key", json!([-1])),
        ("private_key", json!(vec![0; 32])),
        ("created", json!(-1)),
        ("sign_count", json!(4294967296u64)),
    ] {
        let mut changed = value.clone();
        changed[key] = replacement;
        let encoded = encode_unchecked(&changed);
        assert!(decode_passkey_storage_value(&encoded).is_err(), "{key}");
        assert!(
            inspect_passkey_storage_value(&encoded).unwrap().is_err(),
            "{key}"
        );
    }
    for input in ["not base64", "YWJj", "{\"type\":\"passkey\"}"] {
        assert!(inspect_passkey_storage_value(input).is_none());
        assert!(decode_passkey_storage_value(input).is_err());
    }
    let encoded = encode_passkey_storage_value(&original).unwrap();
    assert!(decode_passkey_storage_value(&(encoded.clone() + "AAAA")).is_err());
    let oversized = encoded + &"A".repeat(MAX_STORAGE_BYTES);
    assert!(inspect_passkey_storage_value(&oversized).unwrap().is_err());
}

#[test]
fn optional_android_metadata_defaults_and_unknown_fields_are_accepted() {
    let original = decode_passkey_storage_value(fixture("es256.b64").trim()).unwrap();
    let mut value = serde_json::to_value(&original).unwrap();
    value.as_object_mut().unwrap().remove("zone");
    value.as_object_mut().unwrap().remove("sign_count");
    value["user"].as_object_mut().unwrap().remove("reveal_name");
    value["user"]["display_name"] = Value::Null;
    value["rp"]["name"] = Value::Null;
    value["future"] = json!({"unknown": true});
    let decoded = decode_passkey_storage_value(&encode_unchecked(&value)).unwrap();
    assert_eq!(decoded.zone, "UTC");
    assert_eq!(decoded.sign_count, 0);
    assert!(!decoded.user.reveal_name);
    assert!(decoded.user.display_name.is_none());
    assert!(decoded.rp.name.is_none());
}

#[test]
fn imports_reject_unrepresentable_credentials_and_ambiguous_containers() {
    let original: Value = serde_json::from_str(&fixture("es256.cxf.json")).unwrap();
    let container = json!({"accounts": [{"items": [{"credentials": [original.clone()]}]}]});
    assert!(import_cxf_passkey_json(&container.to_string()).is_ok());
    assert!(import_cxf_passkey_json(
        &json!({"credentials": [original.clone(), original.clone()]}).to_string()
    )
    .is_err());
    for (key, replacement) in [
        ("credentialId", json!("AQID")),
        ("userHandle", json!("")),
        ("rpId", json!("../example.com")),
        ("type", json!("basic-auth")),
        ("fido2Extensions", json!({"credBlob": "AQID"})),
        ("key", json!("not a private key")),
    ] {
        let mut changed = original.clone();
        changed[key] = replacement;
        assert!(
            import_cxf_passkey_json(&changed.to_string()).is_err(),
            "{key}"
        );
    }
    let p384 = PKey::from_ec_key(
        EcKey::generate(&EcGroup::from_curve_name(Nid::SECP384R1).unwrap()).unwrap(),
    )
    .unwrap();
    let rsa3072 = PKey::from_rsa(Rsa::generate(3072).unwrap()).unwrap();
    for key in [p384, rsa3072] {
        let mut changed = original.clone();
        changed["key"] = json!(URL_SAFE_NO_PAD.encode(key.private_key_to_pkcs8().unwrap()));
        assert!(import_cxf_passkey_json(&changed.to_string()).is_err());
    }
}
