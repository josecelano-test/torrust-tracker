use aquatic_udp_protocol::AnnounceRequest;

use crate::protocol::info_hash::InfoHash;

// struct AnnounceRequest {
//     pub connection_id: i64,
//     pub transaction_id: i32,
//     pub info_hash: InfoHash,
//     pub peer_id: PeerId,
//     pub bytes_downloaded: Bytes,
//     pub bytes_uploaded: Bytes,
//     pub bytes_left: Bytes,
//     pub event: AnnounceEvent,
//     pub ip_address: Option<Ipv4Addr>,
//     pub key: u32,
//     pub peers_wanted: u32,
//     pub port: Port
// }

pub struct AnnounceWrapper {
    pub announce_request: AnnounceRequest,
    pub info_hash: InfoHash,
}

impl AnnounceWrapper {
    #[must_use]
    pub fn new(announce_request: &AnnounceRequest) -> Self {
        AnnounceWrapper {
            announce_request: announce_request.clone(),
            info_hash: InfoHash(announce_request.info_hash.0),
        }
    }
}
