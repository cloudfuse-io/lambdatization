use std::env::var;

pub(crate) struct ChappyConf {
    pub virtual_subnet: String,
    pub virtual_ip: String,
}

impl ChappyConf {
    pub(crate) fn load() -> Self {
        Self {
            virtual_subnet: var("CHAPPY_VIRTUAL_SUBNET").unwrap(),
            virtual_ip: var("CHAPPY_VIRTUAL_IP").unwrap(),
        }
    }
}
