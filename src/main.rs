extern crate byteorder;
extern crate bytes;
extern crate clap;
extern crate fs2;
extern crate reqwest;
extern crate serde;
extern crate tokio;
extern crate tokio_stream;
extern crate tokio_util;
extern crate lazy_static;
extern crate warp;

use std::error::Error;
use tokio::net::{TcpStream};
use futures::pin_mut;
use futures::{future, SinkExt, StreamExt, try_join};
use tokio_util::codec::{FramedRead, FramedWrite};
use futures::sink;
use ergvein_protocol::message::*;
mod codec;
use std::time::{SystemTime, UNIX_EPOCH};
use codec::*;
use tokio_stream::wrappers::UnboundedReceiverStream;
use tokio::sync::mpsc;
use rand::{thread_rng, Rng};

use std::net::{Ipv4Addr, SocketAddr};
use std::str::FromStr;
use tokio::net::UdpSocket;
use tokio::runtime::Runtime;

use trust_dns_client::udp::UdpClientStream;
use trust_dns_client::client::{Client, AsyncClient, ClientHandle};
use trust_dns_client::rr::{DNSClass, Name, RData, Record, RecordType};
use trust_dns_client::op::ResponseCode;
use trust_dns_client::rr::rdata::key::KEY;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let (out_sender, out_reciver) = mpsc::unbounded_channel::<Message>();
    let msg_stream = UnboundedReceiverStream::new(out_reciver);
    let mut inmsgs_err = msg_stream.map(Ok);

    let (in_sender, _) = mpsc::unbounded_channel::<Message>();
    let msg_sink = sink::unfold(in_sender, |in_sender, msg| async move {
        println! ("{}" , msg);
        in_sender.send(msg).unwrap();
        Ok::<_, ergvein_protocol::message::Error>(in_sender)
    });
    pin_mut!(msg_sink);


    let ver_msg = build_version_message();
    out_sender.send(Message::Version(ver_msg.clone()))?;
    out_sender.send(Message::VersionAck)?;

    let mut s = TcpStream::connect("188.244.4.78:8667").await?;
    let (r, w) = s.split();
    let mut sink = FramedWrite::new(w, MessageCodec::default());
    
    let mut stream = FramedRead::new(r, MessageCodec::default()).filter_map(|i| match i {
        Ok(i) => {
            println! ("{}" , i);
            future::ready(Some(Ok(i)))
        },
        Err(e) => {
            println!("Failed to read from socket; error={}", e);
            future::ready(Some(Err(e)))
        }
     
    });
    try_join!(sink.send_all(&mut inmsgs_err), msg_sink.send_all(&mut stream))?;
    Ok(())
}
async fn try_dns () {
    let stream = UdpClientStream::<UdpSocket>::new(([127,0,0,1], 24141).into());
    let client = AsyncClient::connect(stream);
    let (mut client, bg) = client.await.expect("connection failed");
    tokio::spawn(bg);
    let query = client.query(Name::from_str("www.example.com.").unwrap(), DNSClass::IN, RecordType::A);
    let response = query.await.expect("connection failed");
    println! ("{:?}", response);
    if let &RData::A(addr) = response.answers()[0].rdata() {
        assert_eq!(addr, Ipv4Addr::new(93, 184, 216, 34));
    }
}

fn  build_version_message() -> VersionMessage {
    // "standard UNIX timestamp in seconds"
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time error")
        .as_secs();

    // "Node random nonce, randomly generated every time a version packet is sent. This nonce is used to detect connections to self."
    let mut rng = thread_rng();
    let nonce: [u8; 8] = rng.gen();

    // Construct the message
    VersionMessage {
        version: Version::current(),
        time: timestamp,
        nonce,
        scan_blocks: vec![],
    }
}