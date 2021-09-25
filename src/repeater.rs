//  Repeater Actor 
//  Used to send notifications ot clients, namely subsribers or listeners
//  THis is a router that resends messages to multiple subscribers 
use actix::{Actor, Context, Handler, Message, Recipient};
use std::collections::HashSet;
use super::NewComment;

//  Struct with a listeners field of the HashSet type that contains Recipient instances 
pub struct RepeaterActor { 
    listeners: HashSet<Recipient<RepeaterUpdate>>, 
    //  The Recipient type is an address that supports only one type Of MEssages
}
//  Add a constructor that creates an empty HashSet: 
impl RepeaterActor { 
    pub fn new() -> Self { 
        Self { 
            listeners: HashSet::new()
        }
    }
}
/*
 Implement an Actor trait for this Struct 
 It's enough to have a standard Context type as an associated context type of Actor, because it can work asynchronously
*/
impl Actor for RepeaterActor {
    type Context = Context<Self>;
}

/*
 Updating the message
 Add a RepeaterUpdate Struct that wraps a NewComment type: 
*/
#[derive(Clone)]
pub struct RepeaterUpdate(pub NewComment);
/*
 We derivee the Clone Trait, because we need to clone this message to resend it to multiple subscrbers 
*/

// Implementing the Message trait for the RepeaterUpdagte struct.
//  WE use an empty type for the REsult associated type, because we don't care about the delivery of these
impl Message for RepeaterUpdate { 
    type Result = ();
}
impl Handler<RepeaterUpdate> for RepeaterActor { 
    type Result = ();

    //  Iterate over all listeners and sends a cloned message to them
    //  Actor receives a message and immediately sends it to all known listeners
    fn handle(&mut self, msg: RepeaterUpdate, _: &mut Self::Context) -> Self::Result {
        for listener in &self.listeners { 
            listener.do_send(msg.clone()).ok();
        }
    }
}
//  Control Message 
//  Actors will send their own Recipient addresses to start listening for updates or to stop any notifications about new comments
pub enum RepeaterControl { 
    Subscribe(Recipient<RepeaterUpdate>),
    Unsubscribe(Recipient<RepeaterUpdate>)
}
//  Implement the Message trait for the RepeaterControl Struct to turn it into the message type and use an empty Result associated type: 
impl Message for RepeaterControl { 
    type Result = ();
}
impl Handler<RepeaterControl> for RepeaterActor { 
    type Result = ();

    fn handle(&mut self, msg: RepeaterControl, _: &mut Self::Context) -> Self::Result { 
        match msg { 
            //  This adds a new Recipient set on the Subcribe message variant, and removes the Recipient upon Unsubscr
            RepeaterControl::Subscribe(listener) => { 
                self.listeners.insert(listener);
            }
            RepeaterControl::Unsubscribe(listener) => { 
                self.listeners.remove(&listener);
            }
        }
    }
}



