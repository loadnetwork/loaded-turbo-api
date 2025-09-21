use anyhow::Error;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use bundles_rs::{ans104::data_item::DataItem, crypto::signer::SignatureType};
use dotenvy::dotenv;
use sha3::{Digest, Keccak256};
use std::env;

// constants
pub(crate) const OBJECT_SIZE_LIMIT: usize = 1_073_741_824; // 1GB
pub(crate) const SERVER_PORT: u32 = 3000;
pub(crate) const DATA_CACHES: &str = "https://gateway.s3-node-1.load.network";
pub(crate) const FAST_FINALITY_INDEXES: &str = "https://gateway.s3-node-1.load.network";

pub(crate) fn get_env_var(key: &str) -> Result<String, Error> {
    dotenv().ok();
    Ok(env::var(key)?)
}

pub(crate) fn extract_owner_address(dataitem: &DataItem) -> String {
    match dataitem.signature_type {
        SignatureType::Arweave => {
            // 512-byte RSA modulus to base64url
            URL_SAFE_NO_PAD.encode(&dataitem.owner)
        }
        SignatureType::Ed25519 => {
            // 32-byte Ed25519 key to base58
            bs58::encode(&dataitem.owner).into_string()
        }
        SignatureType::Ethereum => {
            // 65-byte uncompressed key to EOA
            ethereum_address_from_pubkey(&dataitem.owner)
        }
        _ => {
            // fallback
            URL_SAFE_NO_PAD.encode(&dataitem.owner)
        }
    }
}

fn ethereum_address_from_pubkey(pubkey: &[u8]) -> String {
    if pubkey.len() == 65 && pubkey[0] == 0x04 {
        let hash = Keccak256::digest(&pubkey[1..]);
        let address = &hash[12..];
        format!("0x{}", hex::encode(address))
    } else {
        format!("0x{}", hex::encode(pubkey))
    }
}

pub(crate) fn reconstruct_dataitem_data(data: Vec<u8>) -> Result<(DataItem, String), Error> {
    use byteorder::{LittleEndian, ReadBytesExt};
    use std::io::Read;

    let mut cursor = std::io::Cursor::new(&data);

    // parse the dataitem structure manually
    let signature_type =
        bundles_rs::crypto::signer::SignatureType::from_u16(cursor.read_u16::<LittleEndian>()?);

    let mut signature = vec![0u8; signature_type.signature_len()];
    cursor.read_exact(&mut signature)?;

    let mut owner = vec![0u8; signature_type.owner_len()];
    cursor.read_exact(&mut owner)?;

    // skip target and anchor parsing for now
    // TODO
    let target = None;
    let anchor = None;
    let tags = vec![];

    // read remaining data
    let mut remaining_data = Vec::new();
    cursor.read_to_end(&mut remaining_data)?;

    // create DataItem without verification
    let dataitem =
        DataItem { signature_type, signature, owner, target, anchor, tags, data: remaining_data };

    let di = dataitem.clone();
    let content_type_tag = di
        .tags
        .iter()
        .find(|tag| tag.name.to_lowercase() == "content-type")
        .map(|tag| tag.value.clone())
        .unwrap_or_else(|| "application/octet-stream".to_string());

    Ok((dataitem, content_type_tag))
}
