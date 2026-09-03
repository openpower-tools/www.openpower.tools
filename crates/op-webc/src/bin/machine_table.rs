//! Prints the switch-like interaction machine's full transition table
//! as JSON, derived from the machine itself, for the interaction report
//! generator (`tools/interaction_report`). Run with
//! `cargo run -p op-webc --bin machine_table`.

use op_webc::machine::{Effect, Flight, Input, table};

fn flight(f: Flight) -> &'static str {
    match f {
        Flight::Idle => "Idle",
        Flight::Toward => "Toward",
        Flight::Back => "Back",
    }
}

fn input(i: Input) -> &'static str {
    match i {
        Input::Attend => "Attend",
        Input::Neglect => "Neglect",
        Input::Activate => "Activate",
        Input::Finished => "Finished",
    }
}

fn effect(e: &Effect) -> String {
    match e {
        Effect::SetOn(v) => format!("\"SetOn({v})\""),
        Effect::Attention(v) => format!("\"Attention({v})\""),
        Effect::Arm => "\"Arm\"".to_owned(),
        Effect::Disarm => "\"Disarm\"".to_owned(),
        Effect::Describe(d) => format!("\"Describe({d:?})\""),
    }
}

fn main() {
    let rows: Vec<String> = table()
        .iter()
        .map(|(from, i, to, effects)| {
            format!(
                "{{\"from\":{{\"on\":{},\"attention\":{},\"flight\":\"{}\"}},\"input\":\"{}\",\"to\":{{\"on\":{},\"attention\":{},\"flight\":\"{}\"}},\"effects\":[{}]}}",
                from.on,
                from.attention,
                flight(from.flight),
                input(*i),
                to.on,
                to.attention,
                flight(to.flight),
                effects.iter().map(effect).collect::<Vec<_>>().join(",")
            )
        })
        .collect();
    println!("[{}]", rows.join(",\n"));
}
