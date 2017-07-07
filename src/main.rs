extern crate telegram_bot as tgb;

extern crate hyper;
extern crate hyper_tls;
extern crate futures;
extern crate tokio_core;

extern crate json;

use futures::{Future, Stream};
use futures::future;

use hyper::{Uri, Method, Error};
use hyper::client::{HttpConnector, Request};
use hyper::header::{
    ContentLength
};

use std::str::FromStr;

use tgb::ListeningAction;

const HELP: &'static str =
r#"*Ask The Caterpillar* is a harm reduction chatbot that allows people easy access to information about substances so that they can make _informed_ choices.

You can try things like...

- _Tell me about marijuana_
- _What are the effects of speed?_
- _How much DMT should I take?_
- _Can I mix MDMA and MXE?_
- _How long does PCP last?_
- _What color is the marquis test for LSD?_
- _Is alcohol toxic?_
- _Is cocaine safe?_"#;

const HAVING_ISSUES: &'static str = "We're having some issues on our end, please check by again later!";

struct AskTheCaterpillar {
    api: tgb::Api,

    core: tokio_core::reactor::Core,
    client: hyper::client::Client<hyper_tls::HttpsConnector<HttpConnector>>
}

impl AskTheCaterpillar {
    fn new () -> Result<AskTheCaterpillar, tgb::Error> {
        let api = tgb::Api::from_env("TELEGRAM_BOT_TOKEN")?;

        let core = tokio_core::reactor::Core::new().unwrap();
        let handle = core.handle();

        let client = hyper::Client::configure()
           .connector(hyper_tls::HttpsConnector::new(4, &handle).unwrap())
           .build(&handle);

        Ok(AskTheCaterpillar {
            api, core, client
        })
    }

    pub fn request<'a, T>(&mut self, user_query: T) -> Option<json::JsonValue>
        where T: Into<String> + Clone
    {
        let uri = Uri::from_str(
            "https://www.askthecaterpillar.com/query"
        ).unwrap();

        let mut req = Request::new(Method::Post, uri);

        let query: String = vec![
            r#"--atc"#,
            r#"Content-Disposition: form-data; name="query""#,
            r#""#,
            &user_query.clone().into(),
            r#"--atc--"#,
            r#""#
        ].join("\r\n");

        req.headers_mut().set_raw(
            "content-type",
            "multipart/form-data; boundary=atc"
        );
        req.headers_mut().set(ContentLength(query.len() as u64));

        req.headers_mut().set_raw("accept", "*/*");

        req.set_body(query);

        let work = self.client
        .request(req)
        .and_then(|res| {
            res.body()
                .fold(Vec::new(), |mut v, chunk| {
                    v.extend(&chunk[..]);
                    future::ok::<_, Error>(v)
                })
                .and_then(|chunks| {
                    let s = String::from_utf8(chunks).unwrap();
                    future::ok::<_, Error>(s)
                })
        });

        match self.core.run(work) {
            Ok(val_raw) => {
                match json::parse(&val_raw) {
                    Ok(val_json) => Some(val_json),
                    _ => None
                }
            },
            _ => None
        }
    }

    fn emit(&self, envelope: &tgb::Message, msg: String) -> Result<(), tgb::Error> {
        self.api.send_message(
            envelope.chat.id(),
            msg,
            Some(tgb::ParseMode::Markdown),

            None, None, None
        )?;

        Ok(())
    }

    fn get_atc_payload(&self, atc_data: &Option<json::JsonValue>) -> Option<String> {
        match atc_data {
            &Some(ref data) =>
                Some(
                    data["data"]["messages"][0]["content"]
                        .as_str().unwrap_or(r#""""#)
                        .to_string()
                ),
            &None => None
        }
    }

    fn handle_message(&mut self, envelope: &tgb::Message) -> Result<Option<()>, tgb::Error> {
        if let tgb::MessageType::Text(ref msg) = envelope.msg {
            if msg == "/start" || msg == "/help" {
                if msg == "/start" {
                    self.emit(
                        &envelope,

                        format!("Welcome, {}!", envelope.from.first_name)
                    )?;
                }

                self.emit(&envelope, format!("{}", HELP))?;
            } else {
                let res = self.request(msg.to_string());

                self.emit(
                    &envelope,

                    self.get_atc_payload(&res)
                        .unwrap_or(HAVING_ISSUES.to_string())
                )?;
            }
        }

        Ok(Some(()))
    }

    fn init (&mut self) -> Result<(), tgb::Error> {
        let listener = self.api.listener(tgb::ListeningMethod::LongPoll(None));

        let (sender, receiver) = listener.channel();

        let ack = || sender.send(Ok(ListeningAction::Continue));

        loop {
            let event = receiver.recv();

            if event.is_ok() {
                if let Some(ref envelope) = event.unwrap().message {
                    let _ = self.handle_message(envelope);
                }
            }

            let _ = ack();
        }
    }
}

fn main () {
    let mut atc = AskTheCaterpillar::new().unwrap();

    let _ = atc.init();
}