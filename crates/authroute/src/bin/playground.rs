use authroute_api::{Subject, sample_subject, TargetRef};
use regex::Regex;
fn main() {
    println!("AuthRoute policy playground — type `:help` for commands.\n");

    let subject = sample_subject();
    print_subject(&subject);

    let target : TargetRef = TargetRef{
        group: "gateway.networking.k8s.io".to_string(),
        kind: authroute_api::TargetRefKind::HttpRoute,
        name: "grafana".to_string()
    };
}

fn print_subject(subject: &Subject) {
    println!(
        "  subject: user={:?} groups={:?} email={:?} name={:?}",
        subject.username, subject.groups, subject.email, subject.name
    );
}
