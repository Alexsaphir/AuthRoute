use std::io::{self, Write};

use authroute_api::{Subject, compile_cel_policy, sample_subject};

fn main() {
    println!("AuthRoute policy playground — type `:help` for commands.\n");

    let mut subject = sample_subject();
    let stdin = io::stdin();
    let mut line = String::new();

    print_subject(&subject);
    println!();

    loop {
        print!("policy> ");
        io::stdout().flush().ok();

        line.clear();
        match stdin.read_line(&mut line) {
            Ok(0) => break, // EOF (Ctrl-D)
            Ok(_) => {}
            Err(e) => {
                eprintln!("read error: {e}");
                break;
            }
        }

        let input = line.trim();
        if input.is_empty() {
            continue;
        }

        // Lines starting with `:` are REPL commands; everything else is a CEL
        // expression. CEL policies naturally begin with `user`/`groups`, so we
        // can't let bare keywords double as commands.
        let Some(cmd) = input.strip_prefix(':') else {
            evaluate(input, &subject);
            continue;
        };

        match cmd.split_once(char::is_whitespace) {
            Some(("user", rest)) => {
                subject.username = rest.trim().to_string();
                print_subject(&subject);
            }
            Some(("groups", rest)) => {
                subject.groups = rest.split(',').map(|g| g.trim().to_string()).collect();
                print_subject(&subject);
            }
            Some(("email", rest)) => {
                subject.email = rest.trim().to_string();
                print_subject(&subject);
            }
            Some(("name", rest)) => {
                subject.name = rest.trim().to_string();
                print_subject(&subject);
            }
            _ => match cmd.trim() {
                "help" => print_help(),
                "subject" => print_subject(&subject),
                "reset" => {
                    subject = sample_subject();
                    print_subject(&subject);
                }
                "quit" | "exit" => break,
                other => println!("  unknown command `:{other}` — try `:help`"),
            },
        }
    }

    println!("bye");
}

/// Compile `expr` and report what the evaluator would decide for `subject`.
fn evaluate(expr: &str, subject: &Subject) {
    match compile_cel_policy(expr) {
        Ok(policy) => match policy.evaluate(subject) {
            Ok(true) => println!("  ALLOW"),
            Ok(false) => println!("  DENY"),
            Err(e) => println!("  eval error: {e}"),
        },
        Err(e) => println!("  compile error: {e}"),
    }
}

fn print_subject(subject: &Subject) {
    println!(
        "  subject: user={:?} groups={:?} email={:?} name={:?}",
        subject.username, subject.groups, subject.email, subject.name
    );
}

fn print_help() {
    println!(
        "any bare line is compiled as CEL and evaluated.\n\
         commands (prefixed with `:`):\n  \
         :user <name>        set subject username\n  \
         :groups a, b, c     set subject groups (comma-separated)\n  \
         :email <addr>       set subject email\n  \
         :name <display>     set subject display name\n  \
         :subject            print the current subject\n  \
         :reset              restore the sample subject\n  \
         :help               show this help\n  \
         :quit / :exit       leave (or Ctrl-D)\n\
         \nexamples:\n  \
         \"admins\" in groups\n  \
         user == \"alice@example.com\""
    );
}
