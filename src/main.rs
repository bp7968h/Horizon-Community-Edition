/////////////////////////////////////////////////////////////////////////////////////////////////////
//                                       Horizon Game Server                                       //
//                                                                                                 //
//  This server software is part of a distributed system designed to facilitate communication      //
//  and data transfer between multiple child servers and a master server. Each child server        //
//  operates within a "Region map" managed by the master server, which keeps track of their        //
//  coordinates in a relative cubic light-year space. The coordinates are stored in 64-bit floats  //
//  to avoid coordinate overflow and to ensure high precision.                                     //
//                                                                                                 //
/////////////////////////////////////////////////////////////////////////////////////////////////////

////////////////////////////////////////////////////////////////////
// Use the mimalloc allocator, which boasts excellent performance //
// across a variety of tasks, while being small (8k LOC)          //
////////////////////////////////////////////////////////////////////
#[cfg(target_os = "linux")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

///////////////////////////////////////////
// Import a few things to get us started //
///////////////////////////////////////////

use plugin_api::Plugin;
// use plugins::English;

// Import some third party crates
use serde_json::Value;
use socketioxide::extract::{Data, SocketRef};
use std::{sync::{Arc, Mutex}, time::Duration, path::Path};
use tokio::{main, task::spawn};
use tracing::info;
use horizon_data_types::*;
use viz::{handler::ServiceHandler, serve, Response, Result, Router, Request, Body};
use uuid::Uuid;
use rand;

// Load the plugins API
extern crate plugin_test_api as plugin_api;
extern crate plugin_test_plugins as plugins;

// Import some custom crates from the crates folder in /src
use TerraForge;
use PebbleVault;

//////////////////////////////////////////////////////////////
//                    !!!! WARNING !!!!                     //
// Import all structs (when we have a ton of structs this   //
// will be very bad but should be fine for now)             //
//////////////////////////////////////////////////////////////

/////////////////////////////////////
// Import the modules we will need //
/////////////////////////////////////

mod events;
mod macros;
mod players;
mod subsystems;
mod plugin_manager;

///////////////////////////////////////////////////////////////
//                    !!!! WARNING !!!!                      //
// on_connect runs every time a new player connects to the   //
// server avoid putting memory hungry code here if possible! //
///////////////////////////////////////////////////////////////

/// Handles new player connections to the server.
///
/// This function is called every time a new player connects to the server. It initializes
/// player data, sets up event listeners, and starts necessary subsystems.
///
/// # Arguments
///
/// * `socket` - A reference to the socket connection for the new player.
/// * `data` - Data received with the connection event.
/// * `players` - A thread-safe reference to the collection of all connected players.
///
/// # Warning
///
/// Avoid putting memory-hungry code in this function as it runs for every new connection.
fn on_connect(socket: SocketRef, Data(data): Data<Value>, players: Arc<Mutex<Vec<Player>>>) {
    // Send an optional event to the player that they can hook into to run some post-connection functions
    socket.emit("connected", true).ok();

    // Fetch ID from socket data
    let id = socket.id.as_str();

    // Display join message in log
    println!("Welcome player {} to the game!", id);

    // Authenticate the user
    let player = Player::new(socket.clone(), id.to_string());
    
    // Init the player-related event handlers
    players::init(socket.clone(), players.clone());

    players.lock().unwrap().push(player);

    // Display player join debug messages
    println!("Player {} added to players list", id);
    println!("Socket.IO connected: {:?} {:?}", socket.ns(), socket.id);

    // Send an optional event to the player that they can hook into to run some post-authentication functions
    socket.emit("auth", true).ok();
         
    ///////////////////////////////////////////////////////////
    //  Setup external event listeners for the more complex  //
    //  systems                                              //
    ///////////////////////////////////////////////////////////
    
    // Initialize extra subsystems to listen to events from the client
    subsystems::core::chat::chat::init(socket.clone(), players.clone());
    subsystems::core::game_logic::init();
    subsystems::core::level_data::init();
    subsystems::core::logging::init();
    subsystems::core::notifications::init();
    subsystems::recipe_smith::src::lib::main();

    // DO NOT INIT SUBSYSTEMS BEYOND THIS POINT
    // Send an optional event to the player that they can hook into to start the game client side
    // This event confirms that the server is fully ready to handle data from the player
    let _ = socket.emit("preplay", true);
    socket.emit("beginplay", true).ok();

}

/// Redirects browser users to the master server dashboard.
///
/// This function handles HTTP GET requests to the root path and redirects
/// the user to the master server's dashboard.
///
/// # Arguments
///
/// * `_req` - The incoming HTTP request (unused in this function).
///
/// # Returns
///
/// A `Result` containing the HTTP response with a 302 redirect status.
async fn redirect_to_master_panel(_req: Request) -> Result<Response> {
    let response = Response::builder()
        .status(302)
        .header("Location", "https://youtu.be/dQw4w9WgXcQ")
        .body(Body::empty())
        .unwrap();
    println!("Someone tried to access this server via a browser, redirecting them to the master dashboard");
    Ok(response)
}

/// The main entry point for the Horizon Game Server.
///
/// This function initializes the server, sets up necessary components,
/// and starts listening for incoming connections.
///
/// # Returns
///
/// A `Result` indicating whether the server started successfully or encountered an error.
#[main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /////////////////////////////
    // SERVER STARTUP SEQUENCE //
    /////////////////////////////

    // Show startup ascii art
    subsystems::core::startup::main();
    
    let recipe_smith_thread = tokio::task::spawn(async {
        subsystems::recipe_smith::src::lib::main();
    });


    // Test the plugins API

    let all_plugins = plugins::plugins();

    println!("saying hello in:");
    for (ref name, ref plugin) in all_plugins.list.iter() {
        let instance = plugin.get_instance();
        println!("\t{}: \"{}\"", name, (*instance).say_hello());
    }

    // Start the plugin Manager thread
    let mut _plugin_manager = spawn(async {
        let mut manager = plugin_manager::PluginManager::new();

        // manager.load_plugins_from_directory("./plugins/").is_err() {
        //     println!("Error: Failed to load plugins from dir");
        // }

        let rx = manager.monitor_directory_for_changes("./plugins").expect("Failed to monitor directory");

        let manager_ref = Arc::new(Mutex::new(manager));
        let manager_handle = Arc::clone(&manager_ref);

        // thread::spawn(move || {
        //     let mut locked_manager = manager_handle.lock().unwrap();
        //     unsafe {
        //         locked_manager.handle_directory_events(rx);
        //     }
        // });

        //loop {
        //    // Example of execution of a plugin
        //    let manager = manager_ref.lock().unwrap();
        //    manager.execute_plugin("English Plugin");
        //
        //    std::thread::sleep(Duration::from_secs(10));
        //}

    });


    // Start the TerraForge thread
    // let _terraforge_thread = spawn(async {
    //     TerraForge::main();
    // });
    
    // Start the PebbleVault thread
    let pebble_vault_thread = spawn(async {
        // Run the initial tests
        if let Err(e) = PebbleVault::tests::run_tests() {
            eprintln!("Error running initial PebbleVault tests: {}", e);
        }

        // Set up parameters for the continuous load test
        let db_path = "continuous_load_test.db";
        let num_objects = 10_000;
        let num_regions = 5;
        let num_operations = 3;
        let interval = Duration::from_secs(300); // Run every 5 minutes

        // Check if the database file already exists
        let db_exists = Path::new(db_path).exists();

        // Create or load the VaultManager
        let mut vault_manager = match PebbleVault::VaultManager::new(db_path) {
            Ok(vm) => vm,
            Err(e) => {
                eprintln!("Error creating/loading VaultManager: {}", e);
                return;
            }
        };

        if db_exists {
            println!("Existing database found. Loading data...");
            // The data is already loaded in the VaultManager constructor
        } else {
            println!("No existing database found. Creating new data...");
            // Create initial regions and objects
            for i in 0..num_regions {
                let region_id = match vault_manager.create_or_load_region([i as f64 * 1000.0, 0.0, 0.0], 500.0) {
                    Ok(id) => id,
                    Err(e) => {
                        eprintln!("Error creating region: {}", e);
                        continue;
                    }
                };

                for _ in 0..num_objects / num_regions {
                    let object_uuid = Uuid::new_v4();
                    let object_type = match rand::random::<u8>() % 3 {
                        0 => "player",
                        1 => "building",
                        _ => "resource",
                    };
                    let x = rand::random::<f64>() * 1000.0 - 500.0;
                    let y = rand::random::<f64>() * 1000.0 - 500.0;
                    let z = rand::random::<f64>() * 1000.0 - 500.0;
                    let data = format!("Object {}", object_uuid);

                    if let Err(e) = vault_manager.add_object(region_id, object_uuid, object_type, x, y, z, &data) {
                        eprintln!("Error adding object: {}", e);
                    }
                }
            }

            // Persist the initial data
            if let Err(e) = vault_manager.persist_to_disk() {
                eprintln!("Error persisting initial data: {}", e);
            }
        }

        // Continuous load testing loop
        loop {
            match PebbleVault::load_test::run_load_test(&mut vault_manager, num_objects, num_regions, num_operations) {
                Ok(_) => println!("Continuous load test completed successfully"),
                Err(e) => eprintln!("Error in continuous load test: {}", e),
            }

            // Wait for the specified interval before running the next test
            tokio::time::sleep(interval).await;
        }
    });

    // Define a place to put new players
    let players: Arc<Mutex<Vec<Player>>> = Arc::new(Mutex::new(Vec::new()));
    let (svc, io) = socketioxide::SocketIo::new_svc();
    let players_clone: Arc<Mutex<Vec<Player>>> = players.clone();

    // Handle new player connections
    io.ns("/", move |socket: SocketRef, data: Data<Value>| {
        println!("Player Connected!");
        on_connect(socket, data, players_clone.clone())
    });
    
    // Create a router to handle incoming network requests
    let app = Router::new()
        .get("/", redirect_to_master_panel) // Handle accessing server from browser
        .any("/*", ServiceHandler::new(svc)); // Any other protocalls go to socket server

    info!("Starting server");
    
    // Define a listener on port 3000
    let listener: tokio::net::TcpListener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Listening on 0.0.0.0:3000");

    // Start the server and handle any errors
    if let Err(e) = serve(listener, app).await {
        println!("{}", e);
    }
    Ok(())
}