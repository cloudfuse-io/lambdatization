use std::env::var;

pub struct ChappyConf {
    pub cluster_id: String,
    pub cluster_size: u32,
    pub connection_timeout_ms: u64,
    pub seed_hostname: String,
    pub seed_port: String,
    pub virtual_ip: String,
}

impl ChappyConf {
    pub(crate) fn load() -> Self {
        Self {
            cluster_id: var("CHAPPY_CLUSTER_ID").unwrap_or_else(|_| String::from("default")),
            cluster_size: var("CHAPPY_CLUSTER_SIZE").unwrap().parse().unwrap(),
            connection_timeout_ms: 3000,
            seed_hostname: var("CHAPPY_SEED_HOSTNAME").unwrap(),

            seed_port: var("CHAPPY_SEED_PORT").unwrap(),
            virtual_ip: var("CHAPPY_VIRTUAL_IP").unwrap(),
        }
    }
}
