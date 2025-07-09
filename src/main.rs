use anyhow::Result;
use clap::Parser;
use std::io::ErrorKind;
use std::io::Read;
// use std::io::Write;
use std::net::IpAddr;
use std::net::SocketAddr;
use std::net::TcpListener;
use std::net::TcpStream;
use std::net::UdpSocket;
use std::thread;
use tracing::Level;
use tracing::error;
use tracing::info;
use tracing::warn;
use tracing_subscriber::FmtSubscriber;

/// Scan test software.
#[derive(Parser, Debug)]
#[command(author = "RikoNaka", version, about, long_about = None)]
struct Args {
    /// The service listen address
    #[arg(short, long, default_value = "0.0.0.0")]
    addr: String,

    /// The tcp listen port
    #[arg(short, long, default_value = "")]
    tcp: String,

    /// The udp listen port
    #[arg(short, long, default_value = "")]
    udp: String,
}

fn port_parser(input: &str) -> Result<Vec<u16>> {
    if input.contains(",") {
        let input_split: Vec<&str> = input.split(",").map(|x| x.trim()).collect();
        let mut ports = Vec::new();
        for s in input_split {
            let port: u16 = s.parse()?;
            if !ports.contains(&port) {
                // unique
                ports.push(port);
            }
        }
        Ok(ports)
    } else {
        let port: u16 = input.parse()?;
        Ok(vec![port])
    }
}

fn init_log_level(level: Level) {
    let subscriber = FmtSubscriber::builder().with_max_level(level).finish();
    tracing::subscriber::set_global_default(subscriber).expect("failed to set subscriber");
}

fn handle_client(mut stream: TcpStream) {
    let mut buf = vec![0; 4096];
    match stream.read(&mut buf) {
        Ok(recv_size) => {
            if recv_size > 0 {
                if let Ok(message) = std::str::from_utf8(&buf[0..recv_size]) {
                    info!("tcp message: [{}]", message.trim_end());
                } else {
                    let buf_slice = &buf[0..recv_size];
                    let mut hex_str = String::new();
                    for b in buf_slice {
                        hex_str += &format!("{:02x}", b);
                    }
                    info!("tcp message: [{}]", hex_str);
                }

                // if let Err(e) = stream.write_all(&buf[0..recv_size]) {
                //     eprintln!("send packet failed: {}", e);
                // }
            } else {
                warn!("tcp message len is 0");
            }
        }
        Err(e) => {
            if e.kind() != ErrorKind::ConnectionReset {
                error!("tcp recv failed: {}", e);
            }
        }
    }
}

fn tcp_listener(addr: IpAddr, port: u16) -> Result<()> {
    let socket_addr = SocketAddr::new(addr, port);
    let listener = TcpListener::bind(socket_addr)?;

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                info!("new client: {}", stream.peer_addr()?);
                thread::spawn(move || {
                    handle_client(stream);
                });
            }
            Err(e) => {
                error!("tcp recv failed: {}", e);
            }
        }
    }

    Ok(())
}

fn udp_listener(addr: IpAddr, port: u16) -> Result<()> {
    let socket_addr = SocketAddr::new(addr, port);
    let socket = UdpSocket::bind(socket_addr)?;
    let mut buf = vec![0; 4096];

    loop {
        match socket.recv_from(&mut buf) {
            Ok((recv_size, src_addr)) => {
                if recv_size > 0 {
                    if let Ok(message) = std::str::from_utf8(&buf[0..recv_size]) {
                        info!("from {}: [{}]", src_addr, message);
                    } else {
                        let buf_slice = &buf[0..recv_size];
                        let mut hex_str = String::new();
                        for b in buf_slice {
                            hex_str += &format!("{:02x}", b);
                        }
                        info!("from {}: [{}]", src_addr, hex_str);
                    }
                } else {
                    warn!("from {}, message len is 0", src_addr);
                }
            }
            Err(e) => {
                if e.kind() == ErrorKind::Interrupted {
                    info!("quitting...");
                    break;
                } else {
                    error!("udp recv data error: {}", e);
                    break;
                }
            }
        }
    }

    Ok(())
}

fn main() {
    init_log_level(Level::INFO);

    let args = Args::parse();
    let addr: IpAddr = args
        .addr
        .parse()
        .expect(&format!("illegal addr input: {}", args.addr));

    let mut handles = Vec::new();
    if args.tcp.len() > 0 {
        let tcp_ports =
            port_parser(&args.tcp).expect(&format!("parse tcp port failed: {}", args.tcp));
        for port in tcp_ports {
            let handle = thread::spawn(move || {
                tcp_listener(addr, port).expect(&format!("start tcp listener failed"));
            });
            handles.push(handle);
        }
    } else if args.udp.len() > 0 {
        let udp_ports =
            port_parser(&args.udp).expect(&format!("parse udp port failed: {}", args.tcp));
        for port in udp_ports {
            let handle = thread::spawn(move || {
                udp_listener(addr, port).expect(&format!("start udp listener failed"));
            });
            handles.push(handle);
        }
    }

    for handle in handles {
        match handle.join() {
            Err(e) => error!("thread panicked: {:?}", e),
            _ => (),
        }
    }
}
