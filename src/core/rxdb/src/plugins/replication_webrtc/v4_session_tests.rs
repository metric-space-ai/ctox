// Included in the existing lane4 real-SCTP fixture module: no second setup path.
use crate::plugins::replication_webrtc::index_mod::v4_session::{
    self, AuthorizedTransferBlob, NativeTransferRouter, TransferBlobAccess, TransferGet,
};
use base64::{engine::general_purpose::STANDARD, Engine};
struct RangeAccess {
    blob: VerifiedBlob,
    current: Arc<std::sync::atomic::AtomicBool>,
}
struct RangeGuard(Arc<std::sync::atomic::AtomicBool>);
impl WebRTCPublicationGuard for RangeGuard {
    fn with_current(&self, publish: &mut dyn FnMut() -> RxResult<()>) -> RxResult<()> {
        if !self.0.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(failure("range revoked"));
        }
        publish()
    }
}
#[async_trait]
impl TransferBlobAccess for RangeAccess {
    async fn resolve(&self, r: &TransferGet) -> RxResult<AuthorizedTransferBlob> {
        if r.reference != self.blob.reference() {
            return Err(failure("range not admitted"));
        }
        Ok(AuthorizedTransferBlob {
            blob: self.blob.clone(),
            mime: "audio/wav".into(),
            publication: Arc::new(RangeGuard(self.current.clone())),
        })
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn e25_native_cold_range_sends_complete_hashed_chunks_and_fences_revocation() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    tokio::time::timeout(Duration::from_secs(180),async {
        let root=tempfile::tempdir().unwrap();let cas=Cas::open(root.path().join("cas")).unwrap();
        let size=v4_session::TRANSFER_CHUNK_BYTES as usize;
        let bytes:Vec<u8>=(0..size*3+97).map(|i|((i*73+i/251)%256) as u8).collect();
        let input=root.path().join("audio");std::fs::write(&input,&bytes).unwrap();let blob=cas.import(input,"d".repeat(64)).await.unwrap();
        let current=Arc::new(std::sync::atomic::AtomicBool::new(true));
        let router=NativeTransferRouter::new(cas,Arc::new(RangeAccess{blob:blob.clone(),current:current.clone()}));
        let request=TransferGet{kind:"transfer_get".into(),request_id:"cold-request".into(),reference:blob.reference().into(),object_kind:"blob".into(),offset:size as u64,limit:size as u64};
        let mut unaligned=request.clone();unaligned.offset+=17;assert!(router.read(unaligned).await.is_err(),"Core must never pre-slice an unaligned range");
        let response=router.read(request.clone()).await.unwrap();
        let a=ReliableSession::new("e25-range".into(),ADMISSION[..6].try_into().unwrap()).unwrap();
        let b=ReliableSession::new("e25-range".into(),ADMISSION[..6].try_into().unwrap()).unwrap();
        let pair=PairFixture::connect(a,b).await;
        let mut incoming=pair.target.take_incoming(Some(Lane::Transfer)).unwrap();
        let receive=async {
            let Some(Incoming::Reliable{bytes:message,delivery:Some(delivery),..})=incoming.recv().await else {panic!("native range missing");};
            let value:serde_json::Value=serde_json::from_slice(&message).unwrap();
            assert_eq!(value["type"],"transfer_data");assert_eq!(value["request_id"],"cold-request");
            assert_eq!(value["ref"],blob.reference());assert_eq!(value["offset"],size);assert_eq!(value["total"],bytes.len());assert_eq!(value["mime"],"audio/wav");
            let data=STANDARD.decode(value["data"].as_str().unwrap()).unwrap();
            assert_eq!(data.len(),65_536,"range responses contain complete 64-KiB wire chunks before client slicing");
            assert_eq!(value["sha256"],format!("{:x}",Sha256::digest(&data)),"chunk digest must cover exactly the transferred bytes");
            assert_eq!(&data[17..159],&bytes[size+17..size+159]);delivery.acknowledge().unwrap();
        };
        let (sent,())=tokio::join!(response.send(&pair.source,WireSeq(91)),receive);sent.unwrap();
        let pending=router.read(request).await.unwrap();current.store(false,std::sync::atomic::Ordering::SeqCst);
        assert!(pending.send(&pair.source,WireSeq(92)).await.is_err(),"permission revoked after CAS read must prevent publication");
        pair.close().await;
        eprintln!("E25_CAS_RANGE full_chunk_bytes=65536 cold_native_sctp=true chunk_sha256_verified=true revoke_fenced=true");
    }).await.expect("native range fixture made no terminal progress within its deadlock budget");
}
