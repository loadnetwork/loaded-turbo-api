use crate::utils::{get_env_var, reconstruct_dataitem_data, extract_owner_address};
use crate::db::{get_upload, store_completed_upload};

use anyhow::Error;
use aws_config::{BehaviorVersion, Region};
use aws_sdk_s3::Client;
use sqlx::SqlitePool;
use aws_sdk_s3::types::{CompletedPart, CompletedMultipartUpload};

/// Initialize the ~s3@1.0 device connection using the aws s3 sdk.
pub async fn s3_client() -> Result<Client, Error> {
    let config = aws_config::defaults(BehaviorVersion::latest())
        .endpoint_url(get_env_var("AWS_ENDPOINT_URL").unwrap())
        .region(Region::new(get_env_var("AWS_REGION").unwrap()))
        .credentials_provider(aws_sdk_s3::config::Credentials::new(
            get_env_var("AWS_ACCESS_KEY_ID").unwrap(),
            get_env_var("AWS_SECRET_ACCESS_KEY").unwrap(),
            None,
            None,
            "custom",
        ))
        .load()
        .await;

    let s3_config = aws_sdk_s3::config::Builder::from(&config).force_path_style(true).build();
    Ok(Client::from_conf(s3_config))
}

pub(crate) async fn store_signed_dataitem(data: Vec<u8>) -> Result<String, Error> {
    let s3_bucket_name = get_env_var("S3_BUCKET_NAME").unwrap();
    let s3_dir_name = get_env_var("S3_DIR_NAME").unwrap();

    let client = s3_client().await?;
    let dataitem = reconstruct_dataitem_data(data.clone())?;

    let dataitem_id = dataitem.0.arweave_id();
    let key_dataitem: String = format!("{s3_dir_name}/{dataitem_id}.ans104");

    // store it as ans-104 serialized dataitem
    client
        .put_object()
        .bucket(&s3_bucket_name)
        .key(&key_dataitem)
        .body(data.into())
        .content_type(dataitem.1.to_string())
        .send()
        .await?;

    Ok(dataitem_id)
}

/// simple dataitem existence check against its content length being non-zero
pub(crate) async fn does_dataitem_exist(dataitem_id: &str) -> Result<bool, Error> {
    let s3_bucket_name = get_env_var("S3_BUCKET_NAME").unwrap();
    let s3_dir_name = get_env_var("S3_DIR_NAME").unwrap();
    let key_dataitem: String = format!("{s3_dir_name}/{dataitem_id}.ans104");

    let client = s3_client().await?;

    let res = client.head_object().bucket(&s3_bucket_name).key(&key_dataitem).send().await?;

    if res.content_length > Some(0) {
        return Ok(true);
    }

    Ok(false)
}


/// LS3 multipart Upload Functions
pub async fn create_s3_multipart(upload_key: &str) -> Result<String, Error> {
    let s3_bucket_name = get_env_var("S3_BUCKET_NAME")?;
    let client = s3_client().await?;
    
    let response = client
        .create_multipart_upload()
        .bucket(&s3_bucket_name)
        .key(upload_key)
        .send()
        .await?;
    
    Ok(response.upload_id().unwrap_or_default().to_string())
}

pub async fn upload_part_s3(
    upload_key: &str,
    s3_upload_id: &str,
    part_number: i32,
    body: Vec<u8>
) -> Result<String, Error> {
    let s3_bucket_name = get_env_var("S3_BUCKET_NAME")?;
    let client = s3_client().await?;
    
    let response = client
        .upload_part()
        .bucket(&s3_bucket_name)
        .key(upload_key)
        .upload_id(s3_upload_id)
        .part_number(part_number)
        .body(body.into())
        .send()
        .await?;
        
    Ok(response.e_tag().unwrap_or_default().to_string())
}

async fn get_completed_parts(
    client: &aws_sdk_s3::Client,
    bucket: &str,
    upload_key: &str,
    s3_upload_id: &str
) -> Result<Vec<CompletedPart>, Error> {
    let response = client
        .list_parts()
        .bucket(bucket)
        .key(upload_key)
        .upload_id(s3_upload_id)
        .send()
        .await?;
    
    let parts = response.parts()
        .iter()
        .map(|part| {
            CompletedPart::builder()
                .e_tag(part.e_tag().unwrap_or_default())
                .part_number(part.part_number().unwrap_or_default())
                .build()
        })
        .collect();
    
    Ok(parts)
}

pub async fn finalize_multipart_upload(
    pool: &SqlitePool,
    upload_id: &str
) -> Result<String, Error> {
    let upload = get_upload(pool, upload_id).await?;
    let s3_bucket_name = get_env_var("S3_BUCKET_NAME")?;
    let s3_dir_name = get_env_var("S3_DIR_NAME")?;
    let client = s3_client().await?;
    
    // get all completed parts
    let parts = get_completed_parts(&client, &s3_bucket_name, &upload.upload_key, &upload.s3_upload_id).await?;
    
    // complete multipart upload
    client.complete_multipart_upload()
        .bucket(&s3_bucket_name)
        .key(&upload.upload_key)
        .upload_id(&upload.s3_upload_id)
        .multipart_upload(
            CompletedMultipartUpload::builder()
                .set_parts(Some(parts))
                .build()
        )
        .send()
        .await?;
    
    // get the assembled object to extract dataitem ID
    let assembled_object = client
        .get_object()
        .bucket(&s3_bucket_name)
        .key(&upload.upload_key)
        .send()
        .await?;
    
    let body = assembled_object.body.collect().await?.into_bytes().to_vec();
    let (dataitem, content_type) = reconstruct_dataitem_data(body.clone())?;
    let dataitem_id = dataitem.arweave_id();
    
    let owner_address = extract_owner_address(&dataitem);
    
    // store completed upload info before cleanup
    store_completed_upload(pool, upload_id, &dataitem_id, Some(&owner_address)).await?;
    
    // copy to final location with offchain-dataitems naming standard
    let final_key = format!("{}/{}.ans104", s3_dir_name, dataitem_id);
    
    client.copy_object()
        .bucket(&s3_bucket_name)
        .copy_source(format!("{}/{}", s3_bucket_name, upload.upload_key))
        .key(&final_key)
        .content_type(&content_type.to_string())
        .send()
        .await?;
    
    // delete temporary multipart object
    client.delete_object()
        .bucket(&s3_bucket_name)
        .key(&upload.upload_key)
        .send()
        .await?;
    
    // db cleanups
    sqlx::query("DELETE FROM chunks WHERE upload_id = ?")
        .bind(upload_id)
        .execute(pool)
        .await?;
    
    sqlx::query("DELETE FROM uploads WHERE upload_id = ?")
        .bind(upload_id)
        .execute(pool)
        .await?;
    
    Ok(dataitem_id)
}
