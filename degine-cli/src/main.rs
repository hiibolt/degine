use std::env;
use std::io::{self, Read, Write};
use std::process::{Command, Stdio};

fn main() {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() || args[0] == "help" || args[0] == "--help" {
        println!(
            "\
degine METHOD PATH [JSON]
  DEGINE_TOKEN  dg1.<user id>.<secret>   from the account dialog
  DEGINE_URL    default https://degine.hiibolt.com

  get    /me  /facts  /rules  /labels  /asserts  /inbox  /people  /personal-facts
  get    /asserts/{{id}}/for/{{person}}
  get    /facts  /asserts/{{id}}/graph  /asserts/{{id}}/shares
  get    /comments?target_type=fact&target_id={{id}}
  post   /facts  /asserts  /comments  /labels  /facts/{{id}}/derive
  post   /asserts/{{id}}/share  /comments/{{id}}/resolved  /account/token
  put    /facts/{{id}}  /asserts/{{id}}  /comments/{{id}}  /labels
  delete /facts/{{id}}  /labels/{{id}}  /asserts/{{id}}  /comments/{{id}}  /asserts/{{id}}/share/{{email}}

  a body can be a JSON argument or stdin."
        );
        return;
    }
    let token = env::var("DEGINE_TOKEN").unwrap_or_default();
    if token.is_empty() {
        eprintln!("set DEGINE_TOKEN");
        std::process::exit(1);
    }
    let base = env::var("DEGINE_URL").unwrap_or_else(|_| "https://degine.hiibolt.com".into());
    let method = args.remove(0).to_ascii_uppercase();
    let path = args.remove(0);
    let mut body = args.first().cloned().unwrap_or_default();
    if body.is_empty() && method != "GET" && method != "DELETE" {
        let _ = io::stdin().read_to_string(&mut body);
    }
    let url = format!("{base}{path}");
    let auth = format!("Authorization: Bearer {token}");
    let payload = body.trim().to_string();
    let mut cmd = Command::new("curl");
    cmd.args(["-sS", "-w", "\n%{http_code}", "-X", &method, "-H", &auth]);
    if !payload.is_empty() {
        cmd.args(["-H", "content-type: application/json", "--data-binary", &payload]);
    }
    cmd.arg(&url).stderr(Stdio::inherit());
    let out = cmd.output().expect("curl is required");
    let text = String::from_utf8_lossy(&out.stdout);
    let (payload, code) = text.rsplit_once('\n').unwrap_or((text.as_ref(), "0"));
    let _ = io::stdout().write_all(payload.as_bytes());
    if !payload.is_empty() && !payload.ends_with('\n') {
        println!();
    }
    if !out.status.success() || !code.starts_with('2') {
        std::process::exit(1);
    }
}
