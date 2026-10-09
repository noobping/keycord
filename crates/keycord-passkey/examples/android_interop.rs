//! Produces synthetic outputs for the unchanged Android serializer/signing harness.
use keycord_passkey::{
    decode_passkey_storage_value, encode_passkey_storage_value, import_cxf_passkey_json,
};
use std::{env, fs, path::PathBuf};
fn main() {
    let mut args = env::args_os().skip(1);
    let fixtures = PathBuf::from(args.next().expect("fixture directory"));
    let output = PathBuf::from(args.next().expect("output directory"));
    for name in ["es256", "ed25519", "rs256"] {
        let original = fs::read_to_string(fixtures.join(format!("{name}.b64"))).unwrap();
        let credential = decode_passkey_storage_value(original.trim()).unwrap();
        fs::write(
            output.join(format!("{name}.roundtrip.b64")),
            encode_passkey_storage_value(&credential).unwrap(),
        )
        .unwrap();
        let cxf = fs::read_to_string(fixtures.join(format!("{name}.cxf.json"))).unwrap();
        let imported = import_cxf_passkey_json(&cxf).unwrap();
        fs::write(
            output.join(format!("{name}.import.b64")),
            encode_passkey_storage_value(&imported).unwrap(),
        )
        .unwrap();
    }
}
