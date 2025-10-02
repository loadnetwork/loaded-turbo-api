use anyhow::Error;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use bundles_rs::{ans104::data_item::DataItem, crypto::signer::SignatureType};
use byteorder::{LittleEndian, ReadBytesExt};
use dotenvy::dotenv;
use sha3::{Digest, Keccak256};
use std::{env, io::Read};

// constants
pub(crate) const OBJECT_SIZE_LIMIT: usize = 4 * 1_073_741_824; // 4 GB
pub(crate) const SERVER_PORT: u32 = 3000;
pub(crate) const DATA_CACHES: &str = "https://gateway.s3-node-1.load.network";
pub(crate) const FAST_FINALITY_INDEXES: &str = "https://gateway.s3-node-1.load.network";
pub(crate) const UPLOADER_AR_ADDRESS: &str = "2BBwe2pSXn_Tp-q_mHry0Obp88dc7L-eDIWx0_BUfD0"; // load-s3-agent address
pub(crate) const FREE_UPLOAD_LIMIT_BYTES: u32 = 1048576;
pub(crate) const RECEIPT_VERSION: &str = "0.2.0";
// ported from https://github.com/ardriveapp/turbo-upload-service/blob/main/src/constants.ts#L298
pub(crate) const CHUNK_MIN_SIZE: usize = 1024 * 1024 * 5; // 5MiB - AWS minimum
pub(crate) const CHUNK_MAX_SIZE: usize = 1024 * 1024 * 500; // 500MiB // NOTE: S3 cluster supports upto 5GiB
pub(crate) const DEFAULT_CHUNK_SIZE: i64 = 25_000_000; // 25MB
// a 5 years projection based on 2min blocktime,
// counting from block #1764397
pub(crate) const RECEIPT_HEIGHT_DEADLINE: u64 = 3_079_297;

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
    let mut cursor = std::io::Cursor::new(&data);

    // parse signature type and signature
    let signature_type = SignatureType::from_u16(cursor.read_u16::<LittleEndian>()?);
    let mut signature = vec![0u8; signature_type.signature_len()];
    cursor.read_exact(&mut signature)?;

    // parse owner
    let mut owner = vec![0u8; signature_type.owner_len()];
    cursor.read_exact(&mut owner)?;

    // parse target (1 byte presence + 32 bytes if present)
    let target = match cursor.read_u8()? {
        1 => {
            let mut t = [0u8; 32];
            cursor.read_exact(&mut t)?;
            Some(t)
        }
        0 => None,
        _ => return Err(anyhow::anyhow!("Invalid target presence byte")),
    };

    // parse anchor (1 byte presence + 32 bytes if present)
    let anchor = match cursor.read_u8()? {
        1 => {
            let mut a = [0u8; 32];
            cursor.read_exact(&mut a)?;
            Some(a.to_vec())
        }
        0 => None,
        _ => return Err(anyhow::anyhow!("Invalid anchor presence byte")),
    };

    // parse tags
    let tags_bytes_len = cursor.read_u64::<LittleEndian>()? as usize;

    let mut tags_bytes = vec![0u8; tags_bytes_len];
    cursor.read_exact(&mut tags_bytes)?;

    // decode tags from Avro format
    // let tags = bundles_rs::ans104::tags::decode_tags(&tags_bytes)?;
    let tags = vec![];

    // parse actual dataitem's data (remaining bytes)
    let mut data_bytes = Vec::new();
    cursor.read_to_end(&mut data_bytes)?;

    // create parsed DataItem
    let dataitem = DataItem {
        signature_type,
        signature,
        owner,
        target,
        anchor,
        tags: tags.clone(),
        data: data_bytes,
    };

    // extract content type from tags
    let content_type_tag = tags
        .iter()
        .find(|tag| tag.name.to_lowercase() == "content-type")
        .map(|tag| tag.value.clone())
        .unwrap_or_else(|| "application/octet-stream".to_string());

    Ok((dataitem, content_type_tag))
}
