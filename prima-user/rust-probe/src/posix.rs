// This entry is Rust's standard main/lang_start, called by the Prima musl crt.
fn main() -> std::process::ExitCode {
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::net::UdpSocket;
    use std::os::unix::fs::FileExt;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    // The POSIX target uses the x86_64 C floating-point ABI, matching musl.
    let angle = std::hint::black_box(1.25f64);
    assert!((angle.sin() - 0.9489846193555862).abs() < 1e-12);
    let args = std::env::args().collect::<Vec<_>>();
    let mut ok = args.len() == 3
        && args[1].is_empty()
        && args[2] == "中文 std"
        && std::path::Path::new(&args[0]).file_name().unwrap() == "rust-probe";
    ok &= std::env::var("PRIMA_BOOT").as_deref() == Ok("std env");
    for _ in 0..16 {
        let mut allocation = vec![0u8; 64 * 1024];
        allocation[65535] = 0x5a;
        ok &= allocation[65535] == 0x5a;
        std::hint::black_box(&allocation);
    }
    let mut map = std::collections::HashMap::new();
    map.insert("prima", 42);
    ok &= map.get("prima") == Some(&42);
    // SAFETY: this call precedes all secondary threads and no environment
    // iterators or borrowed values remain live.
    unsafe { std::env::set_var("PRIMA_RUST", "posix") };
    ok &= std::env::var("PRIMA_RUST").as_deref() == Ok("posix");
    let cwd = std::env::current_dir().unwrap();
    ok &= cwd.is_absolute();
    // Refuse collisions instead of touching pre-existing host/guest files.
    let directory = std::env::temp_dir().join(format!("pstd{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let original = directory.join("rust");
    let renamed = directory.join("rust2");
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(&original)
        .unwrap();
    file.write_all(b"prima").unwrap();
    file.seek(SeekFrom::Start(0)).unwrap();
    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();
    ok &= contents == "prima" && file.metadata().unwrap().len() == 5;
    // Positional I/O must leave the shared open-file cursor unchanged, including
    // through a duplicated descriptor. Shrink/regrow must not expose old bytes.
    let mut alias = file.try_clone().unwrap();
    alias.write_all_at(b"STD", 1).unwrap();
    let mut positioned = [0; 5];
    file.read_exact_at(&mut positioned, 0).unwrap();
    ok &= &positioned == b"pSTDa";
    file.set_len(2).unwrap();
    alias.set_len(8).unwrap();
    let mut grown = [0xff; 8];
    alias.read_exact_at(&mut grown, 0).unwrap();
    ok &= &grown == b"pS\0\0\0\0\0\0" && file.metadata().unwrap().len() == 8;
    ok &= file.stream_position().unwrap() == 5 && alias.stream_position().unwrap() == 5;
    ok &= alias.read_at(&mut positioned, 1 << 40).unwrap() == 0;
    #[cfg(target_os = "prima")]
    {
        ok &= file.set_len(1 << 40).unwrap_err().raw_os_error() == Some(28);
        ok &= alias.write_at(b"x", 1 << 40).unwrap_err().raw_os_error() == Some(28);
        ok &= file.metadata().unwrap().len() == 8;
    }
    drop(alias);
    #[cfg(target_os = "prima")]
    {
        ok &= file.sync_all().unwrap_err().raw_os_error() == Some(38);
        ok &= std::process::Command::new("/missing").status().is_err();
        // exec itself has a real VFS path even though spawn's vfork/pipe
        // protocol is outside the current profile.
        use std::os::unix::process::CommandExt;
        ok &= std::process::Command::new("/missing").exec().kind() == std::io::ErrorKind::NotFound;
        ok &= std::net::TcpStream::connect("127.0.0.1:1").is_err();
    }
    drop(file);
    std::fs::rename(&original, &renamed).unwrap();
    ok &= std::fs::read_dir(&directory)
        .unwrap()
        .any(|entry| entry.unwrap().file_name() == "rust2");
    std::fs::remove_file(&renamed).unwrap();
    ok &= std::fs::metadata(&renamed).unwrap_err().kind() == std::io::ErrorKind::NotFound;
    std::fs::remove_dir(&directory).unwrap();
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    let addr = socket.local_addr().unwrap();
    socket.send_to(b"udp", addr).unwrap();
    let mut bytes = [0; 8];
    let (length, from) = socket.recv_from(&mut bytes).unwrap();
    ok &= &bytes[..length] == b"udp" && from == addr;
    drop(socket);
    let value = Arc::new(Mutex::new(0));
    let child_value = value.clone();
    thread_local! {
        static LOCAL: std::cell::Cell<u32> = std::cell::Cell::new(std::process::id());
        static DROP: TlsDrop = TlsDrop(std::process::id());
    }
    static DROPPED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    struct TlsDrop(u32);
    impl Drop for TlsDrop {
        fn drop(&mut self) {
            assert_eq!(self.0, std::process::id());
            DROPPED.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }
    LOCAL.with(|local| ok &= local.get() == std::process::id());
    let child = std::thread::spawn(move || {
        DROP.with(|_| {});
        LOCAL.with(|local| {
            local.set(9);
            *child_value.lock().unwrap() = local.get();
        });
        42
    });
    ok &= child.join().unwrap() == 42 && *value.lock().unwrap() == 9;
    LOCAL.with(|local| ok &= local.get() == std::process::id());
    ok &= DROPPED.load(std::sync::atomic::Ordering::SeqCst) == 1;
    // Force the parent to block on a condvar until the child publishes data.
    let pair = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
    let signal = pair.clone();
    let child = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(20));
        *signal.0.lock().unwrap() = true;
        signal.1.notify_one();
    });
    let mut ready = pair.0.lock().unwrap();
    while !*ready {
        ready = pair.1.wait(ready).unwrap();
    }
    drop(ready);
    child.join().unwrap();
    let before_park = Instant::now();
    std::thread::park_timeout(Duration::from_millis(10));
    ok &= before_park.elapsed() >= Duration::from_millis(10);
    let before = Instant::now();
    std::thread::sleep(Duration::from_millis(10));
    ok &= before.elapsed() >= Duration::from_millis(10);
    ok &= std::process::id() != 0;
    println!("RUST|posix=1;fs=1;env=1;udp=1;thread=1|ok={}", u8::from(ok));
    std::process::ExitCode::from(u8::from(!ok))
}
