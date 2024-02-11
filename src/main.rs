//! A simple example of hooking up stdin/stdout to a WebSocket stream.
//!
//! This example will connect to a server specified in the argument list and
//! then forward all data read on stdin to the server, printing out all data
//! received on stdout.
//!
//! Note that this is not currently optimized for performance, especially around
//! buffer management. Rather it's intended to show an example of working with a
//! client.
//!
//! You can use this example together with the `server` example.

use std::{borrow::Cow, env};

use futures_util::{future, pin_mut, SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::{
    connect_async,
    tungstenite::protocol::{frame::coding::CloseCode, CloseFrame, Message},
};

pub fn rgb_string(r: u8, g: u8, b: u8) -> String {
    format!(
        "\x1b[38;2;{};{};{}m",
        r.to_string(),
        g.to_string(),
        b.to_string()
    )
}

#[tokio::main]
async fn main() {
    let connect_addr = env::args().nth(1).unwrap_or_else(|| {
        eprintln!("No address provided.");
        std::process::exit(1)
    });

    let mut user_name = env::args().nth(2).unwrap_or_else(|| {
        eprintln!("No user name provided.");
        std::process::exit(1)
    }).trim().to_string();
    user_name += ": ";

    let url = url::Url::parse(&connect_addr).unwrap();

    let (stdin_send, stdin_receive) = futures_channel::mpsc::unbounded();
    
    tokio::spawn(read_stdin(stdin_send));

    let (ws_stream, _) = connect_async(url).await.expect("Failed to connect");
    println!("WebSocket handshake has been successfully completed");

    let (mut websocket_send, websocket_receive) = ws_stream.split();
    websocket_send.send(Message::Text(user_name)).await.unwrap();
    let stdin_to_ws = stdin_receive.map(Ok).forward(websocket_send);
    let ws_to_stdout = {
        websocket_receive.for_each(|message| async {
            // println!("PR: {:?}", message);
            let msg = message.unwrap();
            if msg.is_close() {

                println!("Close Frame Received. Disconnection Complete.");
                std::process::exit(0);
            }

            let m = format!(
                "{}{}{}\n",
                rgb_string(255, 0, 0),
                msg.to_string(),
                "\x1b[0m",
            )
            .as_bytes()
            .to_vec();
            // println!("{m}");
            tokio::io::stdout().write_all(&m).await.unwrap();
        })
    };

    pin_mut!(stdin_to_ws, ws_to_stdout);
    future::select(stdin_to_ws, ws_to_stdout).await;
}

// Our helper method which will read data from stdin and send it along the
// sender provided.
async fn read_stdin(stdin_send: futures_channel::mpsc::UnboundedSender<Message>) {
    let mut stdin = tokio::io::stdin();
    loop {
        let mut buf = vec![0; 1024];
        let n = match stdin.read(&mut buf).await {
            Err(_) | Ok(0) => break,
            Ok(n) => n,
        };
        buf.truncate(n);
        let dc = "/dc".to_string();
        let message = String::from_utf8(buf.clone()).unwrap();
        if message.trim().len() == 0 {
            ()
        } else {
            if dc == message.trim() {
                let cf = CloseFrame {
                    code: CloseCode::Normal,
                    reason: Cow::Owned("finished".to_string()),
                };

                stdin_send.unbounded_send(Message::Close(Some(cf))).unwrap();
            } else {
                stdin_send.unbounded_send(Message::text(message.trim())).unwrap();
            }
        }
    }
}
