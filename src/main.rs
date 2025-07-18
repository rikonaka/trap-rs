use anyhow::Result;
use clap::Parser;
use std::io::ErrorKind;
use std::io::Read;
use std::io::Write;
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

    /// When receiving data, return the data set in this parameter
    #[arg(short, long, default_value = "null", default_missing_value = "", num_args(0..2))]
    need_return: String,
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

fn handle_client<'a>(mut stream: TcpStream, need_return: String) -> Result<()> {
    let mut buf = vec![0; 4096];
    match stream.read(&mut buf) {
        Ok(recv_size) => {
            if recv_size > 0 {
                if let Ok(message) = std::str::from_utf8(&buf[0..recv_size]) {
                    info!("tcp from {}: [{}]", stream.peer_addr()?, message.trim_end());
                } else {
                    let buf_slice = &buf[0..recv_size];
                    let mut hex_str = String::new();
                    for b in buf_slice {
                        hex_str += &format!("{:02x}", b);
                    }
                    info!("tcp from  message: [{}]", hex_str);
                }

                if need_return != "null" {
                    if need_return.len() > 0 {
                        match stream.write_all(need_return.as_bytes()) {
                            Ok(_) => (),
                            Err(e) => error!("send back data failed: {}", e),
                        }
                    } else {
                        match stream.write_all(&buf[0..recv_size]) {
                            Ok(_) => (),
                            Err(e) => error!("send back data failed: {}", e),
                        }
                    }
                }
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
    Ok(())
}

fn tcp_listener(addr: IpAddr, port: u16, need_return: String) -> Result<()> {
    info!("start tcp listener");
    let socket_addr = SocketAddr::new(addr, port);
    let listener = TcpListener::bind(socket_addr)?;

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                info!("new client: {}", stream.peer_addr()?);
                let need_return = need_return.clone();
                thread::spawn(move || {
                    handle_client(stream, need_return).expect("handle tcp stream failed");
                });
            }
            Err(e) => {
                error!("tcp recv failed: {}", e);
            }
        }
    }

    Ok(())
}

fn udp_listener<'a>(addr: IpAddr, port: u16, need_return: String) -> Result<()> {
    info!("start udp listener");
    let socket_addr = SocketAddr::new(addr, port);
    let socket = UdpSocket::bind(socket_addr)?;
    let mut buf = vec![0; 4096];

    loop {
        match socket.recv_from(&mut buf) {
            Ok((recv_size, src_addr)) => {
                if recv_size > 0 {
                    match String::from_utf8(buf[0..recv_size].to_vec()) {
                        Ok(message) => {
                            info!("udp from {}: [{}]", src_addr, message.trim_end());
                        }
                        Err(_) => {
                            let buf_slice = &buf[0..recv_size];
                            let mut hex_str = String::new();
                            for b in buf_slice {
                                hex_str += &format!("{:02x}", b);
                            }
                            info!("udp from {}: [{}]", src_addr, hex_str);
                        }
                    }
                } else {
                    warn!("udp from {}, but data len is 0", src_addr);
                }

                if need_return != "null" {
                    info!("return udp data [{}] to {}", need_return, src_addr);
                    if need_return.len() > 0 {
                        match socket.send_to(need_return.as_bytes(), src_addr) {
                            Ok(_send_size) => (),
                            Err(e) => error!("send back data failed: {}", e),
                        };
                    } else {
                        match socket.send_to(&buf[0..recv_size], src_addr) {
                            Ok(_send_size) => (),
                            Err(e) => error!("send back data failed: {}", e),
                        };
                    }
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
    let need_return = args.need_return;
    if args.tcp.len() > 0 {
        let tcp_ports =
            port_parser(&args.tcp).expect(&format!("parse tcp port failed: {}", args.tcp));
        for port in tcp_ports {
            let need_return = need_return.clone();
            let handle = thread::spawn(move || {
                tcp_listener(addr, port, need_return).expect(&format!("start tcp listener failed"));
            });
            handles.push(handle);
        }
    } else if args.udp.len() > 0 {
        let udp_ports =
            port_parser(&args.udp).expect(&format!("parse udp port failed: {}", args.tcp));
        for port in udp_ports {
            let need_return = need_return.clone();
            let handle = thread::spawn(move || {
                udp_listener(addr, port, need_return).expect(&format!("start udp listener failed"));
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
