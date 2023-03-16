use std::env::var;

pub struct ChappyConf {
    pub seed_hostname: String,
    pub seed_port: String,
    pub virtual_ip: String,
}

impl ChappyConf {
    pub(crate) fn load() -> Self {
        Self {
            seed_hostname: var("CHAPPY_SEED_HOSTNAME").unwrap(),
            seed_port: var("CHAPPY_SEED_PORT").unwrap(),
            virtual_ip: var("CHAPPY_VIRTUAL_IP").unwrap(),
        }
    }
}
