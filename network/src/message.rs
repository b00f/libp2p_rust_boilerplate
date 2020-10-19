pub mod greeting;

use serde_cbor::from_slice;
use serde_cbor::to_vec;
use serde::{Deserialize, Serialize};

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub enum Message {
    Greeting(String),
}


impl Into<Vec<u8>> for Message {
    fn into(self) -> Vec<u8> {
        to_vec(&self).unwrap()
    }
}

impl Into<Message> for Vec<u8> {
    fn into(self) -> Message {
        from_slice(&self).unwrap()
    }
}
