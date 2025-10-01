use crate::utils::get_env_var;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use bundles_rs::crypto::arweave::ArweaveSigner;
use rand::rngs::OsRng;
use rsa::{
    pss::BlindedSigningKey,
    signature::{RandomizedSigner, SignatureEncoding},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnsignedReceipt {
    pub id: String,
    pub deadline_height: u64,
    pub timestamp: u64,
    pub version: String,
    // the DataItem owner
    pub owner: String,
    pub data_caches: Vec<String>,
    pub fast_finality_indexes: Vec<String>,
    pub winc: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignedReceipt {
    #[serde(flatten)]
    pub receipt: UnsignedReceipt,
    // the bundler (uploader) public key
    // used to verify the signature
    pub public: String,
    // signature consists of the hash of
    // hash(id + version + timestamp + public key + deadlineHeight + owner)
    pub signature: String,
}

// Deep hash implementation
fn deep_hash(chunks: &[&[u8]]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    for chunk in chunks {
        hasher.update((chunk.len() as u64).to_be_bytes());
        hasher.update(chunk);
    }
    hasher.finalize().to_vec()
}

fn prepare_hash(receipt: &UnsignedReceipt) -> Vec<u8> {
    deep_hash(&[
        receipt.version.as_bytes(),
        receipt.id.as_bytes(),
        receipt.deadline_height.to_string().as_bytes(),
        receipt.timestamp.to_string().as_bytes(),
    ])
}
/// the function's logic follow the signReceipt.ts logic in https://github.com/ardriveapp/turbo-upload-service/blob/main/src/utils/signReceipt.ts
/// excluding the Bundlr/Irys backward-compatibility
pub fn sign_receipt(receipt: UnsignedReceipt) -> Result<SignedReceipt, Box<dyn std::error::Error>> {
    let jwk_str = get_env_var("UPLOADER_JWK")?;
    let jwk = ArweaveSigner::from_jwk_str(&jwk_str)?.to_jwk()?;

    // private key components
    let n = URL_SAFE_NO_PAD.decode(&jwk.n)?;
    let e = URL_SAFE_NO_PAD.decode(&jwk.e)?;
    let d = URL_SAFE_NO_PAD.decode(jwk.d.as_ref().ok_or("Missing private key")?)?;
    let p = URL_SAFE_NO_PAD.decode(jwk.p.as_ref().ok_or("Missing p")?)?;
    let q = URL_SAFE_NO_PAD.decode(jwk.q.as_ref().ok_or("Missing q")?)?;

    // recreate RSA private key
    let n_big = rsa::BigUint::from_bytes_be(&n);
    let e_big = rsa::BigUint::from_bytes_be(&e);
    let d_big = rsa::BigUint::from_bytes_be(&d);
    let primes = vec![rsa::BigUint::from_bytes_be(&p), rsa::BigUint::from_bytes_be(&q)];

    let private_key = rsa::RsaPrivateKey::from_components(n_big, e_big, d_big, primes)?;

    // 1- prepare hash
    let hash = prepare_hash(&receipt);

    // 2- sign with salt 0
    let signing_key = BlindedSigningKey::<Sha256>::new_with_salt_len(private_key, 0);
    let mut rng = OsRng;
    let signature_obj = signing_key.sign_with_rng(&mut rng, &hash);

    // 3- convert to base64url
    let signature = URL_SAFE_NO_PAD.encode(signature_obj.to_bytes());
    let public = jwk.n;

    Ok(SignedReceipt { receipt, public, signature })
}
