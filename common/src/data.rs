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
    pub password: U256
}

impl CastToRequest<RequestUser> for User {
    fn as_request(& self) -> RequestUser {
        RequestUser {
            user_id: Some(self.user_id),
            user_name: Some(self.user_name.clone()),
            password: Some(self.password)
        }    
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RequestUser {
    pub user_id: Option<u32>,
    pub user_name: Option<String>,
    pub password: Option<U256>
}

impl RequestUser {
    pub fn new(user_id:u32, user_name:String, password:U256) -> RequestUser {
        RequestUser {
            user_id: Some(user_id),
            user_name: Some(user_name),
            password: Some(password)
        }
    }
}

impl RequestToFull<User> for RequestUser {
    fn try_to_full(& self) -> Option<User> {
        if let Some(user_id) = self.user_id {
            if let Some(user_name) = &self.user_name {
                if let Some(password) = self.password {
                    return Some(User {
                        user_id,
                        user_name: user_name.clone(),
                        password
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
pub struct TokenCarrier {
    pub token: Uuid,
    pub device_id: u8
}

impl TokenCarrier {
    pub fn new(token: Uuid, device_id: u8) -> TokenCarrier {
        TokenCarrier { token, device_id }
    }
}