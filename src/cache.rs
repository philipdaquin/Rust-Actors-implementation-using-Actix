use actix::prelude::*;
use failure::Error;
use futures::Future;
use redis::{Commands, Client, RedisError};

//  Our actor has to keep an instance of CLient. WE well be using multpile actors for handling parallel request to a database 
pub struct CacheActor { 
    client: Client,
    expiration: usize,
    //  TTL period
}

//  Uses provided address string to create a Client instance, and adds both the client and expiration values to the CacheActor struct 
impl CacheActor { 
    pub fn new(addr: &str, expiration: usize) -> Self { 
        let client = Client::open(addr).unwrap();

        Self { 
            client, expiration
        }
    }
}
// Actor 
impl Actor for CacheActor { 
    type Context = SyncContext<Self>;
}
//  Messages
//  To interact with CacheActor, we have to add two types of messages: to set a value and to get a value 

//  Which provides a pair of key and new value for caching 
struct SetValue { 
    pub path: String,
    pub content: Vec<u8>
}
//  Setting a value message
impl Message for SetValue { 
    type Result = Result<(), RedisError>;
}
//  CacheACtor has support for receiving SetValue messages 
impl Handler<SetValue> for CacheActor { 
    type Result = Result<(), RedisError>;

    fn handle(&mut self, msg: SetValue, _: &mut Self::Context) -> Self::Result { 
        self.client.set_ex(msg.path, msg.content, self.expiration)
        //  We used a Client intance stored in CacheActor to executre the SETEX command from Redis with the set_ex method call 
    }
}

//  This represents a message to extract a value from Redis by key
struct GetValue { 
    pub path: String 
}
//  Get value message 
impl Message for GetValue { 
    type Result = Result<Option<Vec<u8>>, RedisError>; 
}
impl Handler<GetValue> for CacheActor { 
    type Result = Result<Option<Vec<u8>>, RedisError>;

    //  CacheActor also implements a Handler trait for the GetValue message type, and uses the GET command of Redis Storage by calling the get method of Client to extract a value from storage: 
    fn handle(&mut self, msg: GetValue, _: &mut Self::Context) -> Self::Result { 
        self.client.get(&msg.path)
    }
}
//  We need a special type that allows methods to interact with the CacheActor instance 
//  Linking Actors 
#[derive(Clone)]
pub struct CacheLink  { 
    addr: Addr<CacheActor>,
}
//   The constructor only fills the addr field with an Addr value 
impl CacheLink { 
    pub fn new(addr: Addr<CacheActor>) -> Self { 
        Self { 
            addr
        }
    }
    //  We need a CacheLink wrapping struct to add methods to get access to catching features, but need to hide the implementation details and message interchage

    pub fn get_value(&self, path: &str) -> Box<dyn Future<Item = Option<Vec<u8>>, Error = Error >> { 
        let msg = GetValue { 
            path: path.to_owned(),
        };
        let fut = self.addr.send(msg)
            .from_err::<Error>()
            .and_then(|x| x.map_err(Error::from));
        Box::new(fut)
    //  The function returns this interaction sequence as a boxed Future
    }
    //  The next method is jmplemented in a similar way -- the set_value method sets a new value to a cache by sending a SetValue message to CacheActor
    pub fn set_value(&self, path: &str, value: &[u8]) -> Box<dyn Future<Item = (), Error = Error>> { 
        let msg = SetValue { 
            path: path.to_owned(),
            content: value.to_owned(),
        };
        let fut = self.addr.send(msg)
            .from_err::<Error>()
            .and_then(|x| x.map_err(Error::from));
        Box::new(fut)
    }   

}




