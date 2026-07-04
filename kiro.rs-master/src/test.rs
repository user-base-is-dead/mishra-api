use futures::StreamExt;

use crate::debug::{print_event, print_event_verbose, debug_crc, print_hex};
use crate::kiro::model::credentials::KiroCredentials;
use crate::kiro::model::events::Event;
use crate::kiro::model::requests::KiroRequest;
use crate::kiro::parser::EventStreamDecoder;
use crate::kiro::provider::KiroProvider;
use crate::kiro::token_manager::TokenManager;
use crate::model::config::Config;


/// call streaming API and print the return in real time
pub(crate) async fn call_stream_api() -> anyhow::Result<()> {
    // read test.json asrequest body
    let request_body = std::fs::read_to_string("test.json")?;
    println!("request body loaded, length: {} bytes", request_body.len());

    // parserequest bodyas KiroRequest object
    let request: KiroRequest = serde_json::from_str(&request_body)?;
    println!("the parsed request object:");
    println!("  session ID: {}", request.conversation_id());
    println!("  model ID: {}", request.model_id());
    println!("  message contentlength: {} character", request.current_content().len());
    if let Some(ref task_type) = request.conversation_state.agent_task_type {
        println!("  task type: {}", task_type);
    }
    if let Some(ref trigger_type) = request.conversation_state.chat_trigger_type {
        println!("  trigger type: {}", trigger_type);
    }
    println!("  history messagecount: {}", request.conversation_state.history.len());
    println!("  tool count: {}", request.conversation_state.current_message.user_input_message.user_input_message_context.tools.len());

    // load credential
    let credentials = KiroCredentials::load_default()?;
    println!("alreadyload credential");

    // load config
    let config = Config::load_default()?;
    println!("API region: {}", config.region);

    // create TokenManager and KiroProvider
    let token_manager = TokenManager::new(config, credentials);
    let mut provider = KiroProvider::new(token_manager);

    println!("\nstartcall streaming API...\n");
    println!("{}", "=".repeat(60));

    // call streaming API
    let response = provider.call_api_stream(&request_body, None).await?;

    // fetchbyte stream
    let mut stream = response.bytes_stream();
    let mut decoder = EventStreamDecoder::new();

    // handlestreamingdata
    let mut total_bytes = 0usize;
    while let Some(chunk_result) = stream.next().await {
        match chunk_result {
            Ok(chunk) => {
                // debug mode: print the raw hex data
                // println!("\n[receiveddatablock] {} bytes, offset {}", chunk.len(), total_bytes);
                // print_hex(&chunk);
                // debug_crc(&chunk);

                total_bytes += chunk.len();

                // feed the data to the decoder
                if let Err(e) = decoder.feed(&chunk) {
                    eprintln!("[buffererror] {}", e);
                    continue;
                }

                // decode all available frames
                for result in decoder.decode_iter() {
                    match result {
                        Ok(frame) => {
                            // parse event
                            match Event::from_frame(frame) {
                                Ok(event) => {
                                    // concise output
                                    // print_event(&event);
                                    // verbose output (for debugging)
                                    print_event_verbose(&event);
                                }
                                Err(e) => eprintln!("[parse error] {}", e),
                            }
                        }
                        Err(e) => {
                            eprintln!("[frameparse error] {}", e);
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("[network error] {}", e);
                break;
            }
        }
    }

    println!("\n{}", "=".repeat(60));
    println!("streamingresponseend");
    println!("total received {} bytes,decode {} frame", total_bytes, decoder.frames_decoded());

    Ok(())
}