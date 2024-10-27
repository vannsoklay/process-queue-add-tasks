use actix_cors::Cors;
use actix_files::Files;
use actix_web::{web, App, HttpResponse, HttpServer, Responder};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::{self, Receiver, Sender};
use tokio::task;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferTask {
    pub task_id: String,
    pub owner_id: String, // Represents customer ID
    pub retailer_id: String,
    pub product_id: String,
    pub quantity: String,
    pub status: String, // Status field to track task state
}

#[derive(Clone)]
struct AppState {
    sender: Sender<TransferTask>,
    tasks: Arc<Mutex<Vec<TransferTask>>>, // Mutex to store tasks safely
}

async fn process_transfer(task: TransferTask) -> Result<(), String> {
    // Simulate processing the transfer task
    println!(
        "Processing transfer for task ID: {}, owner ID: {}, product ID: {}, quantity: {}",
        task.task_id, task.owner_id, task.product_id, task.quantity
    );
    // Simulate some async work
    tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;

    Ok(())
}

async fn process_queue(data: Arc<Mutex<Vec<TransferTask>>>, mut receiver: Receiver<TransferTask>) {
    while let Some(mut task) = receiver.recv().await {
        let task_id = task.task_id.clone();

        // Spawn async task for processing and removing on completion
        tokio::spawn({
            let tasks = data.clone();
            async move {
                {
                    // Update status to "Processing"
                    let mut tasks = tasks.lock().unwrap();
                    if let Some(t) = tasks.iter_mut().find(|t| t.task_id == task_id) {
                        t.status = "Processing".to_string();
                    }
                }

                // Simulate processing time
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;

                // After processing, update status to "Completed" and remove the task
                let mut tasks = tasks.lock().unwrap();
                if let Some(pos) = tasks.iter().position(|t| t.task_id == task_id) {
                    tasks[pos].status = "Completed".to_string();
                    println!("Task completed and removed: {:?}", tasks[pos]);
                    tasks.remove(pos); // Remove task after marking as completed
                }
            }
        });
    }
}
#[actix_web::post("/submit-task")]
// Submit Task Endpoint
// async fn submit_task(data: web::Data<AppState>, task: web::Json<TransferTask>) -> impl Responder {
//     // Send the task to the queue
//     if let Err(_) = data.sender.send(task.clone()).await {
//         return HttpResponse::InternalServerError().finish();
//     }

//     // Store the task in the tasks vector
//     let mut tasks = data.tasks.lock().unwrap(); // Lock mutex and await
//     tasks.push(task.into_inner());

//     HttpResponse::Ok().json("Task submitted")
// }

async fn submit_task(data: web::Data<AppState>, task: web::Json<TransferTask>) -> impl Responder {
    let mut task = task.into_inner();
    task.status = "Queued".to_string(); // Set initial status to "Queued"

    // Send the task to the queue
    if let Err(_) = data.sender.send(task.clone()).await {
        return HttpResponse::InternalServerError().finish();
    }

    // Store the task in the tasks vector
    let mut tasks = data.tasks.lock().unwrap();
    tasks.push(task);

    HttpResponse::Ok().json("Task submitted and queued")
}

async fn check_task_status(
    data: web::Data<AppState>,
    task_id: web::Path<String>,
) -> impl Responder {
    let tasks = data.tasks.lock().unwrap();
    if let Some(task) = tasks.iter().find(|t| t.task_id == *task_id) {
        return HttpResponse::Ok().json(task);
    }
    HttpResponse::NotFound().json("Task not found")
}

// List Tasks Endpoint based on owner_id
async fn list_tasks(data: web::Data<AppState>, owner_id: web::Path<String>) -> impl Responder {
    let tasks = data.tasks.lock().unwrap(); // Lock mutex and await
    println!("tasks {:?}", tasks);
    let filtered_tasks: Vec<&TransferTask> = tasks
        .iter()
        .filter(|task| task.owner_id == *owner_id)
        .collect();

    if filtered_tasks.is_empty() {
        return HttpResponse::NotFound().json("No tasks found for this owner ID");
    }

    HttpResponse::Ok().json(filtered_tasks) // Return filtered tasks as JSON
}

#[actix_web::get("/")]
async fn index() -> HttpResponse {
    let html_content = r#"
    <!DOCTYPE html>
    <html lang="en">
    <head>
        <meta charset="UTF-8">
        <meta name="viewport" content="width=device-width, initial-scale=1.0">
        <title>Task Submission</title>
    </head>
    <body>
        <h1>Submit Transfer Task</h1>
        <form id="task-form">
            <label for="task_id">Task ID:</label>
            <input type="text" id="task_id" name="task_id" required>
            <br>
            <label for="owner_id">Owner ID:</label>
            <input type="text" id="owner_id" name="owner_id" required>
            <br>
            <label for="retailer_id">Retailer ID:</label>
            <input type="text" id="retailer_id" name="retailer_id" required>
            <br>
            <label for="product_id">Product ID:</label>
            <input type="text" id="product_id" name="product_id" required>
            <br>
            <label for="quantity">Quantity:</label>
            <input type="number" id="quantity" name="quantity" required>
            <br>
            <label for="quantity">Status:</label>
            <input type="number" id="quantity" name="quantity" required>
            <br>
            <input type="hidden" id="status" name="status" value="Queued">
            <button type="submit">Submit Task</button>
        </form>
        <div id="response"></div>
        <script>
            document.getElementById('task-form').addEventListener('submit', async (event) => {
                event.preventDefault();
                const formData = new FormData(event.target);
                const taskData = Object.fromEntries(formData);
                
                const response = await fetch('http://localhost:8080/submit-task', {
                    method: 'POST',
                    headers: {
                        'Content-Type': 'application/json',
                    },
                    body: JSON.stringify(taskData),
                });

                const result = await response.json();
                document.getElementById('response').innerText = 'Task submitted: ' + JSON.stringify(result);
            });
        </script>
    </body>
    </html>
    "#;

    HttpResponse::Ok()
        .content_type("text/html; charset=utf-8")
        .body(html_content)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // Create a bounded channel for the queue
    let (sender, receiver): (Sender<TransferTask>, Receiver<TransferTask>) = mpsc::channel(100);

    // Setup the AppState
    // Setup the AppState with task storage
    let data = web::Data::new(AppState {
        sender,
        tasks: Arc::new(Mutex::new(Vec::new())), // Initialize tasks vector
    });

    let tasks_clone = data.tasks.clone();
    // Spawn the queue processor
    tokio::spawn(async move {
        process_queue(tasks_clone.clone(), receiver).await; // Limit concurrent tasks to 5
    });

    // Start the HTTP server
    HttpServer::new(move || {
        App::new()
            .app_data(data.clone())
            .wrap(
                Cors::default() // Enable CORS
                    .allow_any_origin() // Allow any origin (modify as needed for production)
                    .allow_any_method() // Allow any method
                    .allow_any_header(), // Allow any header
            )
            .service(index) // Add the index route
            .service(submit_task)
            .route(
                "/check_task_status/{task_id}",
                web::get().to(check_task_status),
            )
            .service(web::resource("/tasks/{owner_id}").route(web::get().to(list_tasks)))
    })
    .bind("127.0.0.1:8080")?
    .run()
    .await?;

    // Await the queue processor to finish (in a real application, this would likely run indefinitely)
    // queue_processor.await.unwrap();

    Ok(())
}
