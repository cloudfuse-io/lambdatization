use chappy_seed::Address;

use quinn::{RecvStream as QuicRecvStream, SendStream as QuicSendStream};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;

// TODO -> use this to track existing connections:
// type RegisteredCallers = Arc<Mutex<HashMap<u16, Address>>>;
type RegisteredAddresses = Arc<
    Mutex<HashMap<Address, UnboundedSender<oneshot::Sender<(QuicRecvStream, QuicSendStream)>>>>,
>;
// OR create a service that generates bidir connection directly

struct ConnectionService {
    registered_addresses: RegisteredAddresses,
}

impl ConnectionService {
    pub fn new() -> Self {
        Self {
            registered_addresses: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn create_client_connection(
        remote_address: Address,
        stream_query_recv: UnboundedReceiver<oneshot::Sender<(QuicRecvStream, QuicSendStream)>>,
    ) {
    }

    pub async fn get_stream_for_client(
        &self,
        remote_address: Address,
    ) -> (QuicRecvStream, QuicSendStream) {
        let tx = {
            let mut map = self.registered_addresses.lock().unwrap();
            if let Some(tx) = map.get(&remote_address) {
                tx.clone()
            } else {
                let (tx, rx) = unbounded_channel();
                map.insert(remote_address.clone(), tx.clone()).unwrap();
                tokio::spawn(Self::create_client_connection(remote_address.clone(), rx));
                tx
            }
        };
        let (query_tx, query_rx) = oneshot::channel();
        tx.send(query_tx).unwrap();
        query_rx.await.unwrap()
    }

    async fn create_server_connection(
        remote_address: Address,
        stream_query_recv: UnboundedReceiver<oneshot::Sender<(QuicRecvStream, QuicSendStream)>>,
    ) {
    }

    pub async fn get_stream_for_server(
        &self,
        remote_address: Address,
    ) -> (QuicRecvStream, QuicSendStream) {
        let tx = {
            let mut map = self.registered_addresses.lock().unwrap();
            if let Some(tx) = map.get(&remote_address) {
                tx.clone()
            } else {
                let (tx, rx) = unbounded_channel();
                map.insert(remote_address.clone(), tx.clone()).unwrap();
                tokio::spawn(Self::create_client_connection(remote_address.clone(), rx));
                tx
            }
        };
        let (query_tx, query_rx) = oneshot::channel();
        tx.send(query_tx).unwrap();
        query_rx.await.unwrap()
    }
}
