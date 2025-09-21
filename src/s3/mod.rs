use crate::utils::{get_env_var, reconstruct_dataitem_data};

use anyhow::{Error, anyhow};
use aws_config::{BehaviorVersion, Region};
use aws_sdk_s3::Client;

/// Initialize the ~s3@1.0 device connection using the aws s3 sdk.
async fn s3_client() -> Result<Client, Error> {
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



