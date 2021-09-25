use actix_web::{
    client, middleware, server, fs, ws, App, Error, Form, HttpMessage,
    HttpRequest, HttpResponse, FutureResponse, Result,
};
use actix::{Actor, Addr, SyncArbiter};
use actix_web::http::{self, header, StatusCode};
use actix_web::middleware::{Finished, Middleware, Response, Started};
use actix_web::middleware::identity::RequestIdentity;
use actix_web::middleware::identity::{CookieIdentityPolicy, IdentityService};
use failure::format_err;
use futures::{IntoFuture, Future, future};
use log::{error, debug};
use serde::{Deserialize, Serialize};
use serde_derive::{Deserialize, Serialize};
use std::cell::RefCell;

mod cache;
use crate::cache::{CacheActor, CacheLink};
mod repeater;
use crate::repeater::{RepeaterActor, RepeaterUpdate};
mod notification;
use crate::notification::{NotificationActor};


fn boxed<I, E, F>(fut: F) -> Box<dyn Future<Item = I, Error = E>> 
    where 
        F: Future<Item = I, Error = E> + 'static, { 
            Box::new(fut)
        }



//  HTTP CLient: GET, POST
//  The handlers of this microservice work as proxies and resend incoming request to other microservices, which will not be available to user directly 
//  To send requests to other microservices, we need an HTTP client. The actix_web crate contains one
//  WE need ot add two functions, Get and Post Requests

// **GET request 
fn get_request(url: &str) -> impl Future<Item = Vec<u8>, Error = Error> { 
    //  ClientRequest has shortcuts that create builders with a preset HTTP method
    //  We call the get method that only sets the Method::GET value to a request that in implemented as the calling method of the ClientRequestBuilder 
    client::ClientRequest::get(url) 
        
        .finish().into_future()
        //  We use finish, because GET request don't comtain a body value 
        //  All these methods return a Result with a ClientRequest instance as a successful value 
        
        .and_then(|req| { 
        //  Since we have a Future value, we can use the and_tehn method to add the next processing step  
                req.send()
                //  The send method createas a SendRequest instance which implements the Future trait and sends a request to a server 
                
                    .map_err(Error::from)
                    
                    .and_then(|resp| resp.body().from_err())
                    //  If a request has sent we can take a MessageBody value with the body method call 
                    //  This method is a part of the HttpMessage trait
                    //  MessageBody also implements a Future trait with a Bytes value and we use the and_then method to extend a chain of futures
                    //  and transform a value frim SendRequest to Bytes 

                    .map(|bytes| bytes.to_vec())
                    //  we use to_vec() method of Bytes to convert it into Vec<u8> and provide this value as a response to a client
        })
}
// **POST request 
fn post_request<T, O>(url: &str, params: T) -> impl Future<Item = O, Error = Error> 
    where  
        T: Serialize,
         O: for <'de> Deserialize<'de> + 'static,  { 
    
    client::ClientRequest::post(url)
    //  The post_request function creates ClientRequestBuilder with teh ost method of ClientRequest adn dfills a form with values from the paras variable 
            .form(params).into_future().and_then(|req| { 
                //  We convert Result into Futur and send a request to a server 
                req.send()
                //  We process a response, but do it another way
                //  We get a status of a response with the status method call of HttpResponse and check whether it's successful, with the is_success method call 
                    .map_err(Error::from).and_then(|resp| {
                        //  we use th ejson method of HttpResponse to get a Future that collects a body and deserializes it from JSON 
                        if resp.status().is_success() { 
                            let fut = resp.json::<O>().from_err();
                            boxed(fut)
                            
                        //  If not successful, we return an error to the client
                        } else  { 
                            error!("Microservice error: {}", resp.status());
                            let fut = Err(format_err!("Microservice Error")).into_future().from_err();
                            boxed(fut)
                        }
                    })
            })
    }
#[derive(Serialize, Deserialize)]
pub struct UserForm { 
    email: String, 
    password: String,
}

//  UserId struct 
#[derive(Deserialize)]
pub struct UserId { 
    id: String,
}

#[derive(Deserialize)]
pub struct AddComment  { 
    pub text: String,
}

#[derive(Serialize, Clone)]
pub struct NewComment  { 
    pub uid: String,
    pub text: String,
}

// **Handlers 
//  Now we can implement hadnlers for every supported path of our microservice we will provide a holisitic API to a client, but 
//  actually, we will use a set of microservices to provide all the necessary services to the client 

//  signup route 
//  The Router microservice uses the /signup route to resent a signup request to a users microservuce bound to the 127.0.0.1:8001 address 
//  This request creates new users with filled from UserForm, passed with a parameter wrapped with the Form Type 
fn signup(params: Form<UserForm>) -> FutureResponse<HttpResponse> {
    //  We call the post_request function that we declared before tot send a POST request to a users microservice 
    let fut = post_request("http://127.0.0.1:8001/signup", params.into_inner())
        .map(|_: ()| { 
            //  If successful, we return a response witha  302 status code 
            HttpResponse::Found()
            //  After this, we also set the LOCATION header to redirect the user to a login form with the header method call of HttpResponseBuilder
            .header(header::LOCATION, "/login.html")
            //  We call finish() to create a HttpResponse from a builder and return it as a boxed Future object 
            .finish()
        });
    Box::new(fut)
}

//  Signin 
//  Allow users to sign in to a microservice with the provided credentials
//  **HttpRequest implements the RequestIdentity trait
fn signin((req, params): (HttpRequest<State>, Form<UserForm>)) -> FutureResponse<HttpResponse> { 
//  HTTPRequest: Need to get access to a shared State Object 
//  Form: we need to extract the UserForm struct from the request body 

//  We can use the post_request , but expect it to return a UserId value in its response 
    let fut = post_request("http://127.0.0.1:8001/signin", params.into_inner())
        .map(move |id: UserId| { 
            //  we can use the Remember Method since HTtpRequest implements the REquest Identity trait and we plugged in IdentityService to app
            req.remember(id.id);
            //  After this, we create a response with a 302 stats code, and redirect users to the /comments.html page 
            HttpResponse::build_from(&req)
            .status(StatusCode::FOUND)
            .header(header::LOCATION, "/comments.html")
            .finish()
        });
        
    Box::new(fut)
}

//  New Comment 
//  This handler allows every user who have signed it to leave a comment 
//  + Send a Newcomment to RepeaterActor which will resend it to any Notifcation Actor instances of connected client
fn new_comment((req, params): (HttpRequest<State>, Form<AddComment>)) -> FutureResponse<HttpResponse> { 

    let repeater = req.state().repeater.clone();

    //  First, we call the identity method of the RequestIdentity trait found in HttpRequest -> this will return the user's ID
    let fut = req.identity()
        .ok_or(format_err!("You have sign-in first!").into())
        .into_future()
        //  We can convert it to Result to make it possible to convert it into Future and return an error if the user is not identified
        .and_then(move |uid| { 
            //  We then use the return User ID value to prepare a request for the comments microservice 
            let new_comment = NewComment { 
                uid,
                text: params.into_inner().text,
                //  we then extract the text field from an AddComment form and create a NewComment Struct with teh user's ID and a comment 
            };

            //  The new comment handler is called when a user adds a new commetn and add an extra step to send a NewComment value to a repeater
            let update = RepeaterUpdate(new_comment.clone());
                repeater    
                    .send(update)
                    .then(move |_| Ok(new_comment))
        })
        .and_then(move |params| { 
            post_request::<_, ()>("http://127.0.0.1:8004/new_comment", params)
        })
        .then(move |_| { 
            let res = HttpResponse::build_from(&req)
                .status(StatusCode::FOUND)
                .header(header::LOCATION, "/comments.html")
                .finish();
            Ok(res)
        });
    Box::new(fut)
}

//  Comments 
//  To view all comments that were created by the previous handler, we have to send a GET request to the comments microservice 
//  with the get_request function that we created before and resend the response data to a client: 

//  Caching the list of comments...
fn comments(req: HttpRequest<State>) -> FutureResponse<HttpResponse> { 
    
    //  Create a Future to get a value from another microservice using the get_request method that we have implemented before 
    let fut = get_request("http://127.0.0.1:8003/list");
    //  Get a reference to state, and call the cache method by passing the /list path, then create a Future instance to obtain a new value 
    let fut = req.state().cache("/list", fut)
        .map(|data| { 
            HttpResponse::Ok().body(data)
        });
    Box::new(fut)
}

//  Counter
//  The handler that prints the total quantity of requests also has quite simple implementation
fn counter(req: HttpRequest<State>) -> String { 
    
    format!("{}", 
        req
        .state().counter.borrow())
        // state method of HttpRequest to get a reference to a State Instance 
        // Since the counter value is stored in RefCell, we use the borrow method to get the value from a cell
        //  Now we will add some middleware that will count every reqest to the microservice 
}

//  Add a handler for HTTP request with a resource method call of App and passed the ws_connect function to its
fn ws_connect(req: &HttpRequest<State>) -> Result<HttpResponse, Error>  {
    let repeater = req.state().repeater.clone().recipient();
    //  Clone address of RepeaterActor, converting it into a Recipient which is then used for creating a NotificationActor instance

    //  To start that actor instance, you have to use the ws::start method that uses the current Request and bootstraps WebsocketContext for this actor
    ws::start(req, NotificationActor::new(repeater))
}

////////////////////////////////////////////////////////////////
    //  State constains a cell with an i64 value to count all request 
    //  By default, it is created with a 0 value
    // #[derive(Default)]
    // struct State(RefCell<i64>);
    //  Using Database Actors
    //  We reuse this struct, but add a CacheLink field to use connections with a CacheActor to get or set cached values 
////////////////////////////////////////////////////////////////

//   Adding Websocketsupport to a server 
pub struct State { 
    counter: RefCell<i64>,
    cache: CacheLink,
    repeater: Addr<RepeaterActor>,
}
//  WE need to create a new constructor because we have to provide a CacheLink Instance with the actual address of the caching actor:
impl State { 
    fn new(cache: CacheLink, repeater: Addr<RepeaterActor>) -> Self  {
        Self {
            counter: RefCell::default(),
            cache,
            repeater
        }
    }
    //  To simply our of caching, we will add the cache method to our State implementation
    //  This method will wrap any provided future with a path and try to extract the cached value 
    fn cache<F>(&self, path: &str, fut: F) -> impl Future<Item = Vec<u8>, Error = Error> 
        where 
            F: Future<Item = Vec<u8>, Error = Error> + 'static, { 
                
                //  We have to use a cloned linked because have to move it to the closure that uses it to store a new valie 
                let link = self.cache.clone();

                let path = path.to_owned();

                //  Extracting the cached value and get a Future that requests a vlaue from the cache
                link.get_value(&path)
                    
                    .from_err::<Error>()
                    //  SInce the method returns an Option, we can use the and_then method to check that the value exists in a cache and return the vlaue to the client
                    .and_then(move |opt| { 
                        if let Some(cached) = opt { 
                            debug!("Cached value used");
                            boxed(future::ok(cached))
                        //  If the value isn't availabe, it will obtain a new one, and afterwards, it receives the store-copied value to cache, and returns 
                        //  the value to the client 
                        } else { 
                            let res = fut.and_then(move |data|  {
                                link.set_value(&path, &data)
                                    .then(move |_|  {
                                        debug!("Cached Updated");
                                        future::ok::<_, Error>(data)
                                    }).from_err::<Error>()
                                        
                            });
                            //  This method wraps the provided Future vaue with another Future trait implementation 
                            boxed(res)
                        }
                    })
            }
}


//  Part of the middleware that will be in the middleware section
//  Uses State to count the total quantity of request 
pub struct Counter;


impl Middleware<State> for Counter { 
    //  Start is called when the request is ready and will be sent to a handler
    fn start(&self, req: &HttpRequest<State>) -> Result<Started> { 
        let value = *req.state().counter.borrow();
        //  We will count all incoming request 
        //  To do this, we get the current counter value from RefCell, add 1, and replace the cell with a new value
        *req.state().counter.borrow_mut() = value + 1;
        //  At the end STARTED::DONE vaue will notify you that the current request will be reused  in the next handler/ middler of the processing chain 
        Ok(Started::Done)
        //  enum Started: Response(return immediately) and Future (return in the futrure)
    }
    //  Response is called after the handler returns a response 
    fn response(&self, _req: &HttpRequest<State>, resp: HttpResponse) -> Result<Response> { 
        //  We return a response without any changes in the Response::Done wrapper.
        //  Response also has a variant, Future, if you want toi return a Future that generates an HttpResponse 
        Ok(Response::Done(resp))
    }
    //  Finish is called when data has been sent to a client 
    fn finish(&self, _req: &HttpRequest<State>, _resq: &HttpResponse) -> Finished  {
        //  WE return a DOne variant of the FInished enum
        Finished::Done
    }
}

fn main() {
    env_logger::init();
    let sys = actix::System::new("router");
     //  Creating a new server with the server::new() method expect a closure toreturn the App instance 
    //  You need to set the number of workers or threads to run actors 
    //  Next call is to bind method binds the server's socket to an address, if no address is found then an Err 
    //  WE call the start method to start the Server Actor => This will return an Addr struct with an address that you can use to send messages to a Server actor instance 

    //  Database Actor 
    let addr = SyncArbiter::start(3, || { 
        CacheActor::new("redis://127.0.0.1:6379", 10)
    });

    let cache = CacheLink::new(addr);

    let repeater = RepeaterActor::new().start();

    server::new( move || {
        let state = State::new(cache.clone(), repeater.clone());
        //  App creation 
        App::with_state(state)
            //  This helps with log request and responses 
            .middleware(middleware::Logger::default())
             //  Helps identify request using identity backend that implements the IdentityPolicy trait   
            .middleware(IdentityService::new(
                    //  CookieIdentityPolicy expects a key with at least 32 bytes 
                    CookieIdentityPolicy::new(&[0; 32])
                    .name("auth-example")
                    .secure(false),
                    ))
            .middleware(Counter)

            //  Scope and Routes 
            //  The next thing to our App instanfce is routing 
            //  The scope method expects a 'prefix' of a path and a closure with a scope as a single argument and creates a scope that can contain subroutes 
            .scope("/api", |scope| {
                //  Here, we create a scope for the /api path prefix and add four rutes using the route method: 
                scope
                //  Note: the route method expects a suffix including: path, method and handler 
                    .route("/signup", http::Method::POST, signup)
                    .route("/signin", http::Method::POST, signin)
                    .route("/new_comment", http::Method::POST, new_comment)
                    .route("/comments", http::Method::GET, comments)
                     //  if a server taes a request for /api/signup with the POST method, it will call the signup function 
            })
            //  Counter Middleware, to count the total quantity of request:  
            .route("/stats/counter", http::Method::GET, counter)
            //  We dont need a scope here since we have only one handler and can call th eroute method directly for the App instanc
            
            .resource("/ws", |r| r.method(http::Method::GET).f(ws_connect))
    
            //  Static files handler
                //  The handler method expects a prefix for a pth and a type that implements the Handler Trait 
            .handler(
                "/",
                fs::StaticFiles::new("./static/").unwrap().index_file("index.html")
                 //  SO if a client send a GET request to a path such as /index.html or /css/styles.css, 
                 // then the Static files handler will send the contents of the corresponding files from the ./static/ local folder
            )
    }).workers(1)
        .bind("127.0.0.1:8080")
        .unwrap()
        .start();

    println!("Started http server: 127.0.0.1:8080");
    //  The server actor won't run until we call run the method of the System instance 
    let _ = sys.run();
}
