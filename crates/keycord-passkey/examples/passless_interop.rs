use keycord_passkey::{decode_passless, parse_cxf_passkey_json, PasskeyFormat};
use std::{env, fs, path::PathBuf};
fn main() {
    let mut args = env::args_os().skip(1);
    let fixtures = PathBuf::from(args.next().unwrap());
    let output = PathBuf::from(args.next().unwrap());
    for name in ["es256", "ed25519", "ed25519-19"] {
        let original = fs::read(fixtures.join(format!("passless-{name}.cbor"))).unwrap();
        let credential = decode_passless(&original).unwrap();
        assert_eq!(credential.sign_count, 37);
        assert_eq!(credential.created, 1700000000);
        assert!(credential.discoverable);
        assert_eq!(credential.extensions.cred_protect, Some(3));
        fs::write(output.join(format!("{name}.roundtrip.cbor")), &original).unwrap();
        if name != "ed25519-19" {
            let input = fs::read_to_string(fixtures.join(format!("{name}.cxf.json"))).unwrap();
            let entry = parse_cxf_passkey_json(&input)
                .unwrap()
                .prepare(PasskeyFormat::Passless)
                .unwrap();
            fs::write(output.join(format!("{name}.import.cbor")), &*entry.contents).unwrap();
        }
    }
}
