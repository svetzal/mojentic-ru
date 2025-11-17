//! Async LLM example demonstrating the agent system.
//!
//! This example shows how to create and use asynchronous LLM agents with the
//! AsyncDispatcher. It demonstrates:
//! - BaseAsyncLLMAgent for LLM-powered agents
//! - AsyncAggregatorAgent for collecting multiple events
//! - AsyncDispatcher for event routing
//! - Event correlation across multiple agents

use async_trait::async_trait;
use mojentic::agents::{AsyncAggregatorAgent, AsyncLlmAgent, BaseAsyncAgent};
use mojentic::async_dispatcher::AsyncDispatcher;
use mojentic::event::Event;
use mojentic::llm::gateways::OllamaGateway;
use mojentic::llm::LlmBroker;
use mojentic::router::Router;
use mojentic::Result;
use serde::{Deserialize, Serialize};
use std::any::{Any, TypeId};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

// Define event types
#[derive(Debug, Clone, Serialize, Deserialize)]
struct QuestionEvent {
    source: String,
    correlation_id: Option<String>,
    question: String,
}

impl Event for QuestionEvent {
    fn source(&self) -> &str {
        &self.source
    }
    fn correlation_id(&self) -> Option<&str> {
        self.correlation_id.as_deref()
    }
    fn set_correlation_id(&mut self, id: String) {
        self.correlation_id = Some(id);
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn clone_box(&self) -> Box<dyn Event> {
        Box::new(self.clone())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FactCheckEvent {
    source: String,
    correlation_id: Option<String>,
    question: String,
    facts: Vec<String>,
}

impl Event for FactCheckEvent {
    fn source(&self) -> &str {
        &self.source
    }
    fn correlation_id(&self) -> Option<&str> {
        self.correlation_id.as_deref()
    }
    fn set_correlation_id(&mut self, id: String) {
        self.correlation_id = Some(id);
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn clone_box(&self) -> Box<dyn Event> {
        Box::new(self.clone())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AnswerEvent {
    source: String,
    correlation_id: Option<String>,
    question: String,
    answer: String,
    confidence: f64,
}

impl Event for AnswerEvent {
    fn source(&self) -> &str {
        &self.source
    }
    fn correlation_id(&self) -> Option<&str> {
        self.correlation_id.as_deref()
    }
    fn set_correlation_id(&mut self, id: String) {
        self.correlation_id = Some(id);
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn clone_box(&self) -> Box<dyn Event> {
        Box::new(self.clone())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FinalAnswerEvent {
    source: String,
    correlation_id: Option<String>,
    question: String,
    answer: String,
    facts: Vec<String>,
    confidence: f64,
}

impl Event for FinalAnswerEvent {
    fn source(&self) -> &str {
        &self.source
    }
    fn correlation_id(&self) -> Option<&str> {
        self.correlation_id.as_deref()
    }
    fn set_correlation_id(&mut self, id: String) {
        self.correlation_id = Some(id);
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn clone_box(&self) -> Box<dyn Event> {
        Box::new(self.clone())
    }
}

// Define response models for LLM agents
#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
struct FactCheckResponse {
    facts: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, schemars::JsonSchema)]
struct AnswerResponse {
    answer: String,
    confidence: f64,
}

// Define agents
struct FactCheckerAgent {
    llm_agent: AsyncLlmAgent,
}

impl FactCheckerAgent {
    fn new(broker: Arc<LlmBroker>) -> Self {
        Self {
            llm_agent: AsyncLlmAgent::new(
                broker,
                "You are a fact-checking assistant. Your job is to provide relevant facts about a question.",
                None,
            ),
        }
    }
}

#[async_trait]
impl BaseAsyncAgent for FactCheckerAgent {
    async fn receive_event_async(&self, event: Box<dyn Event>) -> Result<Vec<Box<dyn Event>>> {
        if let Some(question_event) = event.as_any().downcast_ref::<QuestionEvent>() {
            println!("FactCheckerAgent processing question: {}", question_event.question);

            let prompt = format!(
                "Please provide relevant facts about the following question: {}",
                question_event.question
            );

            let response: FactCheckResponse = self
                .llm_agent
                .generate_object(&prompt, event.correlation_id().map(|s| s.to_string()))
                .await?;

            let fact_check_event = Box::new(FactCheckEvent {
                source: "FactCheckerAgent".to_string(),
                correlation_id: event.correlation_id().map(|s| s.to_string()),
                question: question_event.question.clone(),
                facts: response.facts,
            }) as Box<dyn Event>;

            println!("FactCheckerAgent emitting FactCheckEvent");
            return Ok(vec![fact_check_event]);
        }

        Ok(vec![])
    }
}

struct AnswerGeneratorAgent {
    llm_agent: AsyncLlmAgent,
}

impl AnswerGeneratorAgent {
    fn new(broker: Arc<LlmBroker>) -> Self {
        Self {
            llm_agent: AsyncLlmAgent::new(
                broker,
                "You are a question-answering assistant. Your job is to provide accurate answers to questions.",
                None,
            ),
        }
    }
}

#[async_trait]
impl BaseAsyncAgent for AnswerGeneratorAgent {
    async fn receive_event_async(&self, event: Box<dyn Event>) -> Result<Vec<Box<dyn Event>>> {
        if let Some(question_event) = event.as_any().downcast_ref::<QuestionEvent>() {
            println!("AnswerGeneratorAgent processing question: {}", question_event.question);

            let prompt =
                format!("Please answer the following question: {}", question_event.question);

            let response: AnswerResponse = self
                .llm_agent
                .generate_object(&prompt, event.correlation_id().map(|s| s.to_string()))
                .await?;

            let answer_event = Box::new(AnswerEvent {
                source: "AnswerGeneratorAgent".to_string(),
                correlation_id: event.correlation_id().map(|s| s.to_string()),
                question: question_event.question.clone(),
                answer: response.answer,
                confidence: response.confidence,
            }) as Box<dyn Event>;

            println!("AnswerGeneratorAgent emitting AnswerEvent");
            return Ok(vec![answer_event]);
        }

        Ok(vec![])
    }
}

struct FinalAnswerAgent {
    aggregator: AsyncAggregatorAgent,
    final_answer: Arc<Mutex<Option<FinalAnswerEvent>>>,
}

impl FinalAnswerAgent {
    fn new() -> Self {
        Self {
            aggregator: AsyncAggregatorAgent::new(vec![
                TypeId::of::<FactCheckEvent>(),
                TypeId::of::<AnswerEvent>(),
            ]),
            final_answer: Arc::new(Mutex::new(None)),
        }
    }

    async fn get_final_answer(
        &self,
        correlation_id: &str,
        timeout: Duration,
    ) -> Result<Option<FinalAnswerEvent>> {
        // Wait for all needed events
        self.aggregator.wait_for_events(correlation_id, Some(timeout)).await?;

        // Return the final answer if it exists
        let answer = self.final_answer.lock().await;
        Ok(answer.clone())
    }
}

#[async_trait]
impl BaseAsyncAgent for FinalAnswerAgent {
    async fn receive_event_async(&self, event: Box<dyn Event>) -> Result<Vec<Box<dyn Event>>> {
        println!("FinalAnswerAgent received event: {:?}", event.source());

        // Let the aggregator handle event collection
        let result = self.aggregator.receive_event_async(event.clone_box()).await?;

        // If aggregator returned events, it means we have all needed events
        if result.is_empty() {
            // Still waiting for more events
            return Ok(vec![]);
        }

        // Process the collected events
        println!("FinalAnswerAgent processing collected events");

        // The aggregator has collected the events, retrieve them
        let correlation_id = event.correlation_id().unwrap();
        let events = self
            .aggregator
            .wait_for_events(correlation_id, Some(Duration::from_secs(1)))
            .await?;

        // Extract the specific events
        let mut fact_check_event: Option<&FactCheckEvent> = None;
        let mut answer_event: Option<&AnswerEvent> = None;

        for e in &events {
            if let Some(fce) = e.as_any().downcast_ref::<FactCheckEvent>() {
                fact_check_event = Some(fce);
            } else if let Some(ae) = e.as_any().downcast_ref::<AnswerEvent>() {
                answer_event = Some(ae);
            }
        }

        if let (Some(fce), Some(ae)) = (fact_check_event, answer_event) {
            println!("FinalAnswerAgent has both FactCheckEvent and AnswerEvent");

            // Adjust confidence based on facts
            let mut confidence = ae.confidence;
            if !fce.facts.is_empty() {
                confidence = (confidence + 0.1).min(1.0);
            }

            let final_answer_event = FinalAnswerEvent {
                source: "FinalAnswerAgent".to_string(),
                correlation_id: Some(correlation_id.to_string()),
                question: fce.question.clone(),
                answer: ae.answer.clone(),
                facts: fce.facts.clone(),
                confidence,
            };

            println!("FinalAnswerAgent created FinalAnswerEvent");

            // Store the final answer
            {
                let mut answer = self.final_answer.lock().await;
                *answer = Some(final_answer_event.clone());
            }

            return Ok(vec![Box::new(final_answer_event) as Box<dyn Event>]);
        }

        println!("FinalAnswerAgent missing either FactCheckEvent or AnswerEvent");
        Ok(vec![])
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    println!("=== Async LLM Agent Example ===\n");

    // Initialize the LLM broker
    let gateway = Arc::new(OllamaGateway::new());
    let broker = Arc::new(LlmBroker::new("qwen2.5:3b", gateway, None));

    println!("LLM Broker initialized with model: qwen2.5:3b\n");

    // Create agents
    let fact_checker = Arc::new(FactCheckerAgent::new(broker.clone()));
    let answer_generator = Arc::new(AnswerGeneratorAgent::new(broker.clone()));
    let final_answer_agent = Arc::new(FinalAnswerAgent::new());

    // Create router and register agents
    let mut router = Router::new();
    router.add_route::<QuestionEvent>(fact_checker);
    router.add_route::<QuestionEvent>(answer_generator);
    router.add_route::<QuestionEvent>(final_answer_agent.clone());
    router.add_route::<FactCheckEvent>(final_answer_agent.clone());
    router.add_route::<AnswerEvent>(final_answer_agent.clone());

    println!("Router configured with agents\n");

    // Create and start dispatcher
    let mut dispatcher = AsyncDispatcher::new(Arc::new(router));
    dispatcher.start().await?;

    println!("Dispatcher started\n");

    // Create and dispatch a question event
    let question = "What is the capital of France?";
    println!("Question: {}\n", question);

    let event = Box::new(QuestionEvent {
        source: "ExampleSource".to_string(),
        correlation_id: None,
        question: question.to_string(),
    }) as Box<dyn Event>;

    let correlation_id = uuid::Uuid::new_v4().to_string();
    let mut event_with_id = event.clone_box();
    event_with_id.set_correlation_id(correlation_id.clone());

    println!("Dispatching question event with correlation_id: {}\n", correlation_id);
    dispatcher.dispatch(event_with_id);

    // Give dispatcher a moment to start processing
    println!("Waiting for processing to begin...\n");
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Wait for the final answer
    println!("Waiting for final answer from FinalAnswerAgent...\n");
    match final_answer_agent
        .get_final_answer(&correlation_id, Duration::from_secs(60))
        .await
    {
        Ok(Some(final_answer)) => {
            println!("=== FINAL ANSWER ===");
            println!("Question: {}", final_answer.question);
            println!("Answer: {}", final_answer.answer);
            println!("Confidence: {:.2}", final_answer.confidence);
            println!("\nFacts:");
            for fact in &final_answer.facts {
                println!("  - {}", fact);
            }
        }
        Ok(None) => {
            println!("No final answer was generated");
        }
        Err(e) => {
            println!("Error getting final answer: {}", e);
        }
    }

    // Stop the dispatcher
    println!("\nStopping dispatcher...");
    dispatcher.stop().await?;

    println!("Example completed successfully!");

    Ok(())
}
