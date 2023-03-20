use std::env::var;

pub(crate) fn virtual_subnet() -> Option<ipnet::Ipv4Net> {
    var("CHAPPY_VIRTUAL_SUBNET")
        .map(|v| v.parse().unwrap())
        .ok()
}

pub(crate) fn virtual_ip() -> Option<String> {
    var("CHAPPY_VIRTUAL_IP").ok()
}
