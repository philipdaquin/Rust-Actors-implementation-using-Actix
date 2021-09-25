use actix::{Actor, ActorContext, AsyncContext, Handler, Recipient, StreamHandler}; 
//  ActorContext stopts the method Context isntance from breaking connection with the client 
use actix_web::ws::{Message, ProtocolError, WebsocketContext};
use std::time::{Duration, Instant};
use super::State;
use crate::repeater::{RepeaterControl, RepeaterUpdate};

// I havent used the Handler or StreamHandler for handling messages
// But I would use StreamHandler when the actor has to process alot of messages
// The actux runtime has flow as a Stream to an Actor, you may receive warning s

//  Constants that we wil use for sending ping messages to our clients 
const PING_INTERVAL: Duration = Duration::from_secs(20);
const PING_TIMEOUT: Duration = Duration::from_secs(60);
//  We have to send pings to a client because we have to keep the connection alive since serven often have default timeouts for WebSocket Connections 
//  NGINX will cose the conenction after 60 second if there isn't any activity
//  Browsers don't send pings and only send pongs for incoming pings
//  The Server is responsible for sending pings to clients connected via browsers to prevent disconnecitng through a timeout

//  Notification Actor 
//  last_ping: keep the timestamp of the latest ping 
//  This actor also holds the Recipient address to send RepeaterControl messages  
pub struct NotificationActor  { 
    last_ping: Instant,
    repeater: Recipient<RepeaterControl>,
}

//  Setting the constructor: 
impl NotificationActor { 
    pub fn new(repeater: Recipient<RepeaterControl>) -> Self { 
        Self { 
            last_ping: Instant::now(),
            repeater,
        }
    }
}
//  Implenting the NotifcationActor 
impl Actor for NotificationActor  { 
    type Context =  WebsocketContext<Self, State>;

    //  We create a Subscribe message and send it using RepeaterControl
    //  We add a task that will be executed on PING)INTERVAL and will sned a ping message using theping method of WebsocketContext
    fn started(&mut self, context: &mut Self::Context) { 
        let msg = RepeaterControl::Subscribe(context.address().recipient());
        self.repeater.do_send(msg).ok();
        context.run_interval(PING_INTERVAL, |act, context| {
            //  If the interval is larger than out PING_TIMEOUT value, we will interrupt the connection uisng the stop method of the context
            if Instant::now().duration_since(act.last_ping) > PING_TIMEOUT { 
                context.stop();
                return;
            }
            context.ping("ping");
        });
    }
    //  Stopped method implementation
    // This prepares an unsubcribe event with the same address of the actor and sends it to RepeaterActor
    fn stopped(&mut self, context: &mut Self::Context) { 
        let msg = RepeaterControl::Unsubscribe(context.address().recipient());
        self.repeater.do_send(msg).ok();
    }
}
//  Basic Websocket 
//  Implementing a StreamHandler instance of the ws::Message messages
impl StreamHandler<Message, ProtocolError> for NotificationActor { 
    //  Message type has multiple variants that reflects types of Websocket messages from RFC 6455
    fn handle(&mut self, msg: Message, context: &mut Self::Context) { 
        match msg { 
            //  We use Ping and Pong to update the last_pingfield of the actor's struct and use Close to stop the connection by user's demand 
            Message::Ping(msg) => { 
                self.last_ping = Instant::now();
                context.pong(&msg);
            }
            Message::Pong(_) => { 
                self.last_ping = Instant::now();
            }
            Message::Text(_) => { },
            Message::Binary(_) => { },
            Message::Close(_) => { 
                context.stop();
            }
        }
    }
}

//  This Handler allow us to receive RepeaterUpdate messages
//  and to send NewComment values to a client 
impl Handler<RepeaterUpdate> for NotificationActor { 
    type Result = ();
    
    fn handle(&mut self, msg: RepeaterUpdate, context: &mut Self::Context) -> Self::Result {
        let RepeaterUpdate(comment) = msg;
        //  Destruct a RepeaterUpdate message to get a NewCOmment Value, serialises it to JSON using the serde_json crate
        if let Ok(data) = serde_json::to_string(&comment) { 
            //  data is sent the client using the text method of WebsocketContext
            context.text(data);
        }
    }
}
