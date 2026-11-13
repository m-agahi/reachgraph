//! A discriminator for edge loss through `?` and `.await`. Taken from the
//! usability run of 2026-11-12 (ledger 2061, ADR-1222): without the standard
//! library loaded, `Try` and `Future` are unresolved, so a call whose result
//! goes through `?` lost its edge while the same call written plainly kept it.
use std::future::Future;

pub struct Status(pub String);
pub struct Req {
    pub name: String,
}
pub struct Resp {
    pub msg: String,
}

pub mod v1 {
    pub trait Greeter {
        fn say_hello(
            &self,
            r: super::Req,
        ) -> impl std::future::Future<Output = Result<super::Resp, super::Status>>;
    }
}

pub mod v2 {
    pub trait Greeter2 {
        fn say_hello(
            &self,
            r: super::Req,
        ) -> impl std::future::Future<Output = Result<super::Resp, super::Status>>;
    }
}

pub struct Svc;
pub struct Svc2;

pub async fn run<F: Future>(f: F) -> F::Output {
    f.await
}

fn passthrough(e: Status) -> Status {
    Status(format!("wrapped {}", e.0))
}

fn path_helper(n: &str) -> String {
    n.to_uppercase()
}

fn deep_rule(n: &str) -> bool {
    n.len() > 3
}

fn local_result(n: &str) -> Result<String, Status> {
    if deep_rule(n) {
        Ok(n.into())
    } else {
        Err(Status("short".into()))
    }
}

impl Svc {
    async fn decide(&self, r: &Req) -> Result<String, Status> {
        local_result(&r.name)
    }
}

impl v1::Greeter for Svc {
    async fn say_hello(&self, r: Req) -> Result<Resp, Status> {
        let before = path_helper(&r.name);
        run(async move {
            let decided = self.decide(&r).await?;
            let mapped = local_result(&decided).map_err(|e| passthrough(e))?;
            let inner = path_helper(&mapped);
            Ok(Resp {
                msg: format!("{before}{inner}"),
            })
        })
        .await
    }
}

impl v2::Greeter2 for Svc2 {
    async fn say_hello(&self, r: Req) -> Result<Resp, Status> {
        // The discriminator: the same call, once through `?` and once plain.
        let q = local_result(&r.name)?;
        let plain = local_result(&r.name);
        let x = path_helper(&q);
        let _ = plain;
        Ok(Resp { msg: x })
    }
}

pub fn orphan() -> u8 {
    7
}
