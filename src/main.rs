#![windows_subsystem="windows"] // Hide the console
extern crate reqwest;
use serde_json::{Value, json};
use chrono::{Datelike, Utc, DateTime};
use std::{fs, fs::File, io, io::BufRead, path::Path};

#[derive(PartialEq)]
enum RequestType {
    Get,
    Post
}

#[tokio::main]
async fn main() {
    // Setup canvas client
    let canvas_client = reqwest::Client::new();

    // Get courses for the current semester
    let mut course_json = send_request("https://canvas.instructure.com/api/v1/courses?include[]=total_scores", RequestType::Get, &canvas_client, "1050~5g3LDgBZVnGv5H8mTi0tleLlBt9pRu7861LRkEZ8e93PAoKBHWq8KtIy0YM0uYmk", vec![], None).await;
    for i in (0..course_json.len()).rev() {
        println!("Starting At: {}", course_json[i]["course_code"].to_string());
        let start_date = match DateTime::parse_from_rfc3339(&course_json[i]["start_at"].to_string().replace('"', "")) {
            Ok(date) => date,
            Err(_) => {
                course_json.remove(i);
                continue;
            }
        };
        if start_date.num_days_from_ce() + 160 < Utc::now().num_days_from_ce() { // Asume a semester is roughly 160 days
            course_json.remove(i);
        }
    }

    // Load log file if it exists, else create it
    if !Path::new("canvas_assignments.txt").exists() {
        File::create("canvas_assignments.txt").expect("Failed to create file!");
    }
    let mut prev_assignments: Vec<u64> = vec![];
    if let Ok(lines) = read_lines("canvas_assignments.txt") {
        for line in lines.flatten() {
            if let Ok(id) = line.parse() { 
                prev_assignments.push(id);
            }
        }
    }

    // Get assignments from those courses
    let todoist_client = reqwest::Client::new();
    for course in course_json {
        let assignments = send_request(&format!("https://canvas.instructure.com/api/v1/courses/{}/assignments?page=1&per_page=100", course["id"]), RequestType::Get, &canvas_client, "1050~5g3LDgBZVnGv5H8mTi0tleLlBt9pRu7861LRkEZ8e93PAoKBHWq8KtIy0YM0uYmk", vec![], None).await;
        for assignment in assignments {
            // Check if the assignment is in the log file
            if !prev_assignments.contains(match &assignment["id"].to_string().parse(){ Ok(id) => id, Err(_) => &0}) {
                // Send assignment to todoist
                // Setup body (task name, date string)
                let task_name = format!("{class} {assignment}", class=get_short_class_name(&course["name"].to_string().replace('"', "")), assignment=assignment["name"].to_string().replace('"', "").replace("\\", ""));
                let date = match DateTime::parse_from_rfc3339(&assignment["due_at"].to_string().replace('"', "")) {
                    Ok(date) => date,
                    Err(_) => continue,
                };
                let body = json!({
                    "content": task_name,
                    "due_string": format!("{year}-{month}-{day}", year=date.year(), month=date.month(), day=date.day())
                });
                // Send
                send_request("https://api.todoist.com/rest/v1/tasks", RequestType::Post, &todoist_client, "ed8089f6e191cdb714dad5a0de15ae95786b8b3f", vec![], Some(body)).await;
                // Add assignment to previous assignments
                prev_assignments.push(assignment["id"].to_string().parse().unwrap());
            }
        };
    };

    // Save previous assignments to file    
    let mut joined_assignments = String::new();
    for i in 0..prev_assignments.len() {
        joined_assignments.push_str(&prev_assignments[i].to_string());
        if i < prev_assignments.len() - 1 {joined_assignments.push('\n');}
    }
    match fs::write("canvas_assignments.txt", joined_assignments) {
        Ok(_) => {},
        Err(error) => panic!("Failed to write to save file: {}", error),
    }
}

fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where P: AsRef<Path>, {
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}


fn get_short_class_name(class_name: &str) -> &str {
    for i in 0..class_name.len() {
        let character = class_name.chars().nth(i).unwrap();
        if character == ':' || character == ',' || (i > 0 && class_name.chars().nth(i - 1).unwrap().is_digit(10) && !character.is_digit(10)){
            return &class_name[..i];
        }
    }
    class_name
}

async fn send_request(url: &str, request_type: RequestType, client: &reqwest::Client, bearer_token: &str, headers: Vec<(String, String)>, body: Option<Value>) -> Vec<Value> {
    let mut request_builder = match request_type {
        RequestType::Get => client.get(url),
        RequestType::Post => client.post(url),
    };

    // Add headers
    request_builder = request_builder.bearer_auth(bearer_token);
    for header in headers {
        request_builder = request_builder.header(&header.0, header.1);
    }
    // Add body
    if let Some(body) = body {
        request_builder = request_builder.json(&body);
    }


    let response = request_builder.send().await.expect("Failed to send request!");

    if request_type != RequestType::Get {return vec![];} 
    match serde_json::from_str(&response.text().await.expect("Failed to parse response into string!")) {
        Ok(json) => json,
        Err(_) => vec![]
    }
}