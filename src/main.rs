extern crate reqwest;
#[macro_use] extern crate serde;
use serde_json::{Value, json};
use serde::{Serialize};
use chrono::{DateTime, Datelike, Duration, Utc};
use std::{collections::HashMap, fs, fs::File, path::Path};

#[derive(PartialEq, Debug)]
enum RequestType {
    Get,
    Post,
    Delete
}

#[derive(Serialize, Deserialize, Debug)]
struct Assignment {
    id: String,
    title: String,
    due_date: String,
}

#[tokio::main]
async fn main() {
    // Setup canvas client
    let canvas_client = reqwest::Client::new();

    // Get courses for the current semester
    println!("Getting courses...");
    let mut course_json = send_request("https://canvas.instructure.com/api/v1/courses?per_page=100", RequestType::Get, &canvas_client, "1050~5g3LDgBZVnGv5H8mTi0tleLlBt9pRu7861LRkEZ8e93PAoKBHWq8KtIy0YM0uYmk", vec![], None).await
        .expect("Failed to get courses!");
    let course_json = course_json.as_array_mut().unwrap();
    for i in (0..course_json.len()).rev() {
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
    println!("Loading previous assignments...");
    let mut prev_assignments: HashMap<String, Assignment> = HashMap::new();
    let path = Path::new("/home/jafioti/other/canvas_task_handler/canvas_assignments.txt");
    if !path.exists() {
        File::create(path).expect("Failed to create file!");
    } else if let Ok(content) = fs::read_to_string(path) {
        prev_assignments = serde_json::from_str(&content).unwrap();
    }

    // Get assignments from those courses
    println!("Running through new assignments...");
    let todoist_client = reqwest::Client::new();
    for course in course_json {
        let assignments = send_request(&format!("https://canvas.instructure.com/api/v1/courses/{}/assignments?page=1&per_page=100", course["id"]), RequestType::Get, &canvas_client, "1050~5g3LDgBZVnGv5H8mTi0tleLlBt9pRu7861LRkEZ8e93PAoKBHWq8KtIy0YM0uYmk", vec![], None).await.unwrap();
        for assignment in assignments.as_array().unwrap() {
            // Check if the assignment is in the log file
            if !prev_assignments.contains_key(&assignment["id"].to_string()) || prev_assignments[&assignment["id"].to_string()].due_date != assignment["due_at"] {
                // Send assignment to todoist
                // Setup body (task name, date string)
                let task_name = format!("{class} {assignment}", class=get_short_class_name(&course["name"].to_string().replace('"', "")), assignment=assignment["name"].to_string().replace('"', "").replace("\\", ""));
                let date = match DateTime::parse_from_rfc3339(&assignment["due_at"].to_string().replace('"', "")) {
                    Ok(date) => date,
                    Err(_) => continue,
                } - Duration::hours(5); // Remove 5 hours to stop weird "next-day" glitch
                let body = json!({
                    "content": &task_name,
                    "due_string": format!("{year}-{month}-{day}", year=date.year(), month=date.month(), day=date.day())
                });
                // Send
                send_request("https://api.todoist.com/rest/v1/tasks", RequestType::Post, &todoist_client, "ed8089f6e191cdb714dad5a0de15ae95786b8b3f", vec![], Some(body)).await;
                // Add assignment to previous assignments
                let new_assignment = Assignment {
                    id: assignment["id"].to_string().replace('"', ""),
                    title: assignment["name"].to_string().replace('"', ""),
                    due_date: assignment["due_at"].to_string().replace('"', ""),
                };
                match prev_assignments.get_mut(&new_assignment.id) {
                    Some(past_assignment) => {
                        // Delete old assignment
                        send_request(&format!("https://api.todoist.com/rest/v1/tasks/{}", past_assignment.id), RequestType::Delete, &todoist_client, "ed8089f6e191cdb714dad5a0de15ae95786b8b3f", vec![], None).await;
                        // Change date
                        past_assignment.due_date = assignment["due_at"].to_string().replace('"', "");
                    },
                    None => {prev_assignments.insert(new_assignment.id.clone(), new_assignment);},
                }
            }
        };
    };

    // Save previous assignments to file
    println!("Saving new assignments...");
    match fs::write(path, serde_json::to_string(&prev_assignments).expect("Failed to serialize assignments")) {
        Ok(_) => {},
        Err(error) => panic!("Failed to write to save file: {}", error),
    }
}


fn get_short_class_name(class_name: &str) -> &str {
    for i in 0..class_name.len() {
        let character = class_name.chars().nth(i).unwrap();
        if character == ':' || character == ',' || (i > 0 && class_name.chars().nth(i - 1).unwrap().is_digit(10) && !character.is_digit(10)){
            return if class_name[..i].replace('"', "") == "Section Merge" {
                &class_name[i+2..]
            } else {
                &class_name[..i]
            };
        }
    }
    class_name
}

async fn send_request(url: &str, request_type: RequestType, client: &reqwest::Client, bearer_token: &str, headers: Vec<(String, String)>, body: Option<Value>) -> Option<Value> {
    let mut request_builder = match request_type {
        RequestType::Get => client.get(url),
        RequestType::Post => client.post(url),
        RequestType::Delete => client.delete(url),
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

    if request_type != RequestType::Get {return None;} 
    match serde_json::from_str(&response.text().await.expect("Failed to parse response into string!")) {
        Ok(json) => json,
        Err(_) => None
    }
}