use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use std::fs;
use std::path::Path;

fn main() {
    let mut rng = OsRng;
    let signing_key = SigningKey::generate(&mut rng);
    let verify_key = signing_key.verifying_key();

    let private_key_hex = hex::encode(signing_key.to_bytes());
    let public_key_hex = hex::encode(verify_key.to_bytes());

    let keys_dir = Path::new("keys");
    fs::create_dir_all(keys_dir).expect("failed to create keys directory");

    fs::write(
        keys_dir.join("private_key.txt"),
        format!("private key (add to gateway proxy project.toml):\nsigning_key = '{}'\n", private_key_hex.to_lowercase())
    ).expect("failed to write private key");

    fs::write(
        keys_dir.join("public_key.txt"),
        format!("public key (add to api project.toml):\ngateway_key = '{}'\n", public_key_hex.to_lowercase())
    ).expect("failed to write public key");

    println!("keys generated and saved in the 'keys' directory");
}
