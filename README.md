<p align="center">
  <a href="https://load.network">
    <img src="https://gateway.load.rs/bundle/0x83cf4417880af0d2df56ce04ecfc108ea4ee940e8fb81400e31ab81571e28d21/0">
  </a>
</p>

## About
A Rust-based [turbo-sdk](https://github.com/ardriveapp/turbo-sdk) compatible upload service. This API makes it possible to use Load S3 temporary storage layer to create & store ANS-104 DataItems offchain, temporarily -Load S3- using the Arweave's most used bundling client, `turbo-sdk`.

> Warning: this repository is actively under development and could have breaking changes until reaching full [turbo-upload-service](https://github.com/ardriveapp/turbo-upload-service) API (v1) compatibility in v1.0.0.
>
> In current release, the max object (dataitem) size limit is 4 GB


## Roadmap

| Endpoint  | Status |
| :-------------: |:-------------:|
| `GET /` `GET /info`| ✅ |
| `GET /bundler_metrics` | ✅ (placeholder) |
| `GET /health`| ✅ |
| `GET /v1/tx/{dataitem_id}/offsets` | ✅ (placeholder)|
| `POST /v1/tx/{token}` (<= 10MB uploads)     | ✅     |
| `GET /v1/tx/{dataitem_id}/status` | ✅  |
| `GET /v1/chunks/{token}/-1/-1`      | ✅     |
| `GET /v1/chunks/{token}/{upload_id}/-1`      | ✅   |
| `GET /v1/chunks/{token}/{upload_id}/status`      | ✅    |
| `POST /v1/chunks/{token}/{upload_id}/{offset}`      | ✅    |
| `POST /v1/chunks/{token}/{upload_id}/finalize` | ✅ |
| `GET /account/balance/:id`| not supported, [deprecated](https://github.com/ardriveapp/turbo-upload-service/blob/main/src/router.ts#L48) in turbo-upload-service|
| `GET /price/:token/:byteCount?`| not supported, deprecated in turbo-upload-service|

## Endpoints:

- loaded-turbo-api (offchain, Load S3 bundler endpoint): https://loaded-turbo-api.load.network
- data cache / fast finality index: https://gateway.s3-node-1.load.network

## Examples

### Small uploads (<= 10 MB)

```js
import {
  TurboFactory,
  developmentTurboConfiguration,
} from '@ardrive/turbo-sdk/node';
import Arweave from 'arweave';
import fs from 'fs';

(async () => {
  // Create a test file
  const testData = 'Hello from loaded-turbo-api S3 bundler!';
  fs.writeFileSync('test-file.txt', testData);

  // Create an Arweave key for testing
  const arweave = new Arweave({});
  const jwk = await Arweave.crypto.generateJWK();
  const address = await arweave.wallets.jwkToAddress(jwk);
  console.log('Test wallet address:', address);

  const customTurboConfig = {
    ...developmentTurboConfiguration,
    uploadServiceConfig: {
      url: 'https://loaded-turbo-api.load.network', // loaded-turbo-api endpoint
    },
  };

  // Create authenticated client
  const turboAuthClient = TurboFactory.authenticated({
    privateKey: jwk,
    ...customTurboConfig,
  });

  try {
    // Test upload
    console.log('Uploading file loaded-turbo-api');
    const uploadResult = await turboAuthClient.uploadFile({
    fileStreamFactory: () => fs.createReadStream('test-file.txt'),
    fileSizeFactory: () => fs.statSync('test-file.txt').size,
    dataItemOpts: {
        tags: [
        { name: 'Content-Type', value: 'text/plain' }
        ]
    },
    signal: AbortSignal.timeout(30_000),
    });


    console.log('Upload successful!');
    console.log(JSON.stringify(uploadResult, null, 2));

    // Verify the response structure
    console.log('\n=== Response Validation ===');
    console.log('ID:', uploadResult.id);
    console.log('Owner:', uploadResult.owner);
    console.log('Winc:', uploadResult.winc);
    console.log('Data Caches:', uploadResult.dataCaches);
    console.log('Fast Finality Indexes:', uploadResult.fastFinalityIndexes);

  } catch (error) {
    console.error('Upload failed:', error);
    if (error.response) {
      console.error('Response status:', error.response.status);
      console.error('Response data:', error.response.data);
    }
  } finally {
    fs.unlinkSync('test-file.txt');
  }
})();
```

### Large multipart resumable uploads

```js
import {
  TurboFactory,
  developmentTurboConfiguration,
} from '@ardrive/turbo-sdk/node';
import Arweave from 'arweave';
import fs from 'fs';

(async () => {
  // Create large file to force multipart upload (>10MB)
  const largeTestData = 'A'.repeat(15 * 1024 * 1024); // 15MB
  fs.writeFileSync('large-test-file.txt', largeTestData);
  console.log('Created 15MB test file');

  // Generate test wallet
  const arweave = new Arweave({});
  const jwk = await Arweave.crypto.generateJWK();
  const address = await arweave.wallets.jwkToAddress(jwk);
  console.log('Test wallet address:', address);

  const customTurboConfig = {
    ...developmentTurboConfiguration,
    uploadServiceConfig: {
      url: 'https://loaded-turbo-api.load.network',
    },
  };

  const turboAuthClient = TurboFactory.authenticated({
    privateKey: jwk,
    ...customTurboConfig,
  });

  try {
    console.log('Starting multipart upload...');
    
    const uploadResult = await turboAuthClient.uploadFile({
      fileStreamFactory: () => fs.createReadStream('large-test-file.txt'),
      fileSizeFactory: () => fs.statSync('large-test-file.txt').size,
      dataItemOpts: {
        tags: [{ name: 'Content-Type', value: 'text/plain' }]
      },
      events: {
        onProgress: ({ totalBytes, processedBytes, step }) => {
          const percent = ((processedBytes / totalBytes) * 100).toFixed(1);
          console.log(`${step.toUpperCase()} Progress: ${percent}% (${processedBytes}/${totalBytes})`);
        },
        onUploadProgress: ({ totalBytes, processedBytes }) => {
          console.log(`Upload: ${((processedBytes / totalBytes) * 100).toFixed(1)}%`);
        },
      },
    });

    console.log('Result:', uploadResult);

  } catch (error) {
    console.error('\nUpload failed:', error.message);
    if (error.response) {
      console.error('Status:', error.response.status);
      console.error('Data:', error.response.data);
    }
  } finally {
    fs.unlinkSync('large-test-file.txt');
  }
})();

```

## License

Licensed at your option under either of:
 * [Apache License, Version 2.0](LICENSE-APACHE)
 * [MIT License](LICENSE-MIT)

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.