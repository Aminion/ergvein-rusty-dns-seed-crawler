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
use tokio::net::{TcpListener, TcpStream, ToSocketAddrs};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use futures::future::{AbortHandle, Abortable, Aborted};
use futures::pin_mut;
use futures::{future, Sink, SinkExt, Stream, StreamExt};
use tokio_util::codec::{FramedRead, FramedWrite};
use ergvein_protocol::message::*;
mod codec;
use codec::*;
use tokio_stream::wrappers::UnboundedReceiverStream;
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    {
       let mut stream = TcpStream::connect("188.244.4.78:8667").await?;
       
       let (r, w) = stream.split();
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
       
       let x = stream.for_each (|i| future::ready(())).await;
       let (out_sender, out_reciver) = mpsc::unbounded_channel::<Message>();
       let msg_stream = UnboundedReceiverStream::new(out_reciver);
       //handshake
    }
    
    Ok(())
}


async fn handshake(
    addr: String,
    db: Arc<DB>,
    msg_reciever: &mut mpsc::UnboundedReceiver<Message>,
    msg_sender: &mpsc::UnboundedSender<Message>,
) -> Result<(), IndexerError> {
    let ver_msg = build_version_message(db);
    msg_sender
        .send(Message::Version(ver_msg.clone()))
        .map_err(|e| {
            println!("Error when sending handshake: {:?}", e);
            IndexerError::HandshakeSendError
        })?;
    let timeout = tokio::time::sleep(Duration::from_secs(20));
    tokio::pin!(timeout);
    let mut got_version = false;
    let mut got_ack = false;
    while !(got_version && got_ack) {
        tokio::select! {
            _ = &mut timeout => {
                eprintln!("Handshake timeout {}", addr);
                Err(IndexerError::HandshakeTimeout)?
            }
            emsg = msg_reciever.recv() => match emsg {
                None => {
                    eprintln!("Failed to recv handshake for {}", addr);
                    Err(IndexerError::HandshakeRecv)?
                }
                Some(msg) => match msg {
                    Message::Version(vmsg)=> {
                        if !Version::current().compatible(&vmsg.version) {
                            eprint!("Not compatible version for client {}, version {:?}", addr, vmsg.version);
                            Err(IndexerError::NotCompatible(vmsg.version.clone()))?;
                        }
                        if vmsg.nonce == ver_msg.nonce {
                            eprint!("Connected to self, nonce identical for {}", addr);
                            Err(IndexerError::HandshakeNonceIdentical)?;
                        }
                        println!("Handshaked with client {} and version {:?}", addr, vmsg.version);
                        got_version = true;
                        msg_sender.send(Message::VersionAck).map_err(|e| {
                            println!("Error when sending verack: {:?}", e);
                            IndexerError::HandshakeSendError
                        })?;
                    }
                    Message::VersionAck => {
                        println!("Received verack for client {}", addr);
                        got_ack = true;
                    }
                    _ => {
                        eprintln!("Received from {} something that not handshake: {:?}", addr, msg);
                        Err(IndexerError::HandshakeViolation)?;
                    },
                },
            }
        }
    }
    Ok(())
}