use std::env::var;

pub(crate) struct ChappyConf {
    pub virtual_subnet: String,
}

impl ChappyConf {
    pub(crate) fn load() -> Self {
        Self {
            virtual_subnet: var("CHAPPY_VIRTUAL_SUBNET").unwrap(),
        }
    }
}
