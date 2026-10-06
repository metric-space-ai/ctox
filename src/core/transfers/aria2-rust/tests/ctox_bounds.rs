use aria2_rust::{options::OptionSet, storage::FileStorage};

#[tokio::test]
async fn pinned_length_caps_direct_cache_and_async_write_paths() {
    let dir = tempfile::tempdir().unwrap();
    for cache in ["0", "1M"] {
        let mut opts = OptionSet::with_defaults();
        opts.set("ctox-expected-length", "4");
        opts.set("disk-cache", cache);
        let path = dir.path().join(format!("payload-{cache}"));
        let storage =
            FileStorage::from_opts(path.clone(), 4, aria2_rust::storage::AllocMode::None, &opts);
        storage.ensure().await.unwrap();
        assert!(storage.try_pwrite(3, b"xx").is_err());
        assert!(storage.try_cache(3, b"xx").is_err());
        assert!(storage.write_at(u64::MAX, b"x").await.is_err());
        assert!(storage.write_body(0, b"12345").await.is_err());
        storage.write_body(0, b"1234").await.unwrap();
        storage.flush().await.unwrap();
        assert_eq!(std::fs::read(path).unwrap(), b"1234");
    }
}

#[tokio::test]
async fn portable_socket_writes_preserve_payload_and_sigpipe_protection() {
    use aria2_rust::sockopt;
    use tokio::io::AsyncReadExt;
    use tokio::net::{TcpListener, TcpStream, UdpSocket};

    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap())
            .await
            .unwrap();
        let (mut peer, _) = listener.accept().await.unwrap();
        #[cfg(not(any(target_os = "linux", target_os = "android", target_os = "fuchsia")))]
        assert!(!sockopt::apply_tcp_quickack(&client).unwrap());
        sockopt::send_all(&client, b"scalar").await.unwrap();
        #[cfg(target_vendor = "apple")]
        assert!(rustix::net::sockopt::socket_nosigpipe(&client).unwrap());
        // Reset the option so the vectored path must independently establish it.
        #[cfg(target_vendor = "apple")]
        rustix::net::sockopt::set_socket_nosigpipe(&client, false).unwrap();
        sockopt::writev_all(&client, &[b"vec", b"tored"])
            .await
            .unwrap();
        #[cfg(target_vendor = "apple")]
        assert!(rustix::net::sockopt::socket_nosigpipe(&client).unwrap());
        let mut received = [0; 14];
        peer.read_exact(&mut received).await.unwrap();
        assert_eq!(&received, b"scalarvectored");

        let sender = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let receiver = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        sender
            .connect(receiver.local_addr().unwrap())
            .await
            .unwrap();
        assert_eq!(sockopt::udp_send(&sender, b"datagram").await.unwrap(), 8);
        let mut datagram = [0; 16];
        let size = receiver.recv(&mut datagram).await.unwrap();
        assert_eq!(&datagram[..size], b"datagram");
        #[cfg(target_vendor = "apple")]
        assert!(rustix::net::sockopt::socket_nosigpipe(&sender).unwrap());
    })
    .await
    .expect("loopback socket operations must finish within five seconds");
}
