use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::U256;

pub trait CastToRequest<T> {
    fn as_request(& self) -> T;
}

pub trait RequestToFull<T> {
    fn try_to_full(& self) -> Option<T>;
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct User {
    pub user_id: u32,
    pub user_name: String,
    pub admin: bool
}

impl CastToRequest<RequestUser> for User {
    fn as_request(& self) -> RequestUser {
        RequestUser {
            user_id: Some(self.user_id),
            user_name: Some(self.user_name.clone()),
            admin: Some(self.admin),
            password: None,
            device_id: None,
            token: None
        }    
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RequestUser {
    pub user_id: Option<u32>,
    pub user_name: Option<String>,
    pub admin: Option<bool>,
    pub password: Option<U256>,
    pub device_id: Option<u8>,
    pub token: Option<Uuid>
}

impl RequestUser {
    pub fn new(user_id:u32, user_name:String, password:U256) -> RequestUser {
        RequestUser {
            user_id: Some(user_id),
            user_name: Some(user_name),
            admin: Some(false),
            password: Some(password),
            device_id: None,
            token: None
        }
    }
}

impl RequestToFull<User> for RequestUser {
    fn try_to_full(& self) -> Option<User> {
        if let Some(user_id) = self.user_id {
            if let Some(user_name) = &self.user_name {
                if let Some(admin) = self.admin{
                    return Some(User {
                        user_id,
                        user_name: user_name.clone(),
                        admin
                    });
                }
            }
        }
        
        None
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Device {
    pub device_id: u8,
    pub device_name: String
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RequestDevice {
    pub user_id: Option<u32>,
    pub device_id: Option<u8>,
    pub device_name: Option<String>,
    pub token: Option<Uuid>
}

impl RequestToFull<Device> for RequestDevice {
    fn try_to_full(& self) -> Option<Device> {
        if let Some(device_id) = self.device_id {
            if let Some(name) = &self.device_name {
                return Some(Device { device_id, device_name: name.clone() });
            }
        }

        None
    }
}

#[derive(Debug, PartialEq, Clone, Deserialize, Serialize)]
pub struct TokenCarrier {
    pub token: Uuid,
    pub device_id: Option<u8>
}

impl TokenCarrier {
    pub fn new(token: Uuid, device_id: u8) -> TokenCarrier {
        TokenCarrier { token, device_id: Some(device_id) }
    }

    pub fn new_token(token: Uuid) -> TokenCarrier {
        TokenCarrier { token, device_id: None }
    }

    pub fn get_device_id(& self) -> u8 {
        if let Some(id) = self.device_id {
            return id;
        }

        0_u8
    }

    pub fn token_as_hex_string(& self) -> String {
        crate::bytes_to_hex_string(self.token.as_bytes())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum Reply<T> {
    Ok{ value: T, token: Option<TokenCarrier> },
    NotFound { token: Option<TokenCarrier> },
    Denied { token: Option<TokenCarrier>},
    AuthFailed,
    MissingParameter{ token: Option<TokenCarrier>},
    Error{ token: Option<TokenCarrier>},
    Failed
}

impl<T> Reply<T> {
    pub fn new(value: T) -> Self {
        Self::Ok { value, token: None }
    }
}

pub enum AccessType {
    Read,
    ReadWrite,
    ReadWriteDelete,
    All,
    Owner,
    No
}

impl AccessType {
    pub fn from_str(typ: String) -> AccessType {
        let typ = typ.to_uppercase();
        match typ.as_str() {
            "R" => AccessType::Read,
            "RW" => AccessType::ReadWrite,
            "RWD" => AccessType::ReadWriteDelete,
            "A" => AccessType::All,
            "O" => AccessType::Owner,
            "N" => AccessType::No,
            _ => AccessType::No
        }
    }

    pub fn cast(& self) -> String {
        match self {
            AccessType::Read => "R",
            AccessType::ReadWrite => "RW",
            AccessType::ReadWriteDelete => "RWD",
            AccessType::All => "A",
            AccessType::Owner => "O",
            AccessType::No => "N"
        }.to_string()
    }
}

pub struct Repository {
    pub repo_name: String,
    pub display_name: Option<String>,
    pub game: Option<String>
}