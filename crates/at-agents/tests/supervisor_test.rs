use at_agents::state_machine::AgentState;
use at_agents::supervisor::AgentSupervisor;
use at_core::types::{AgentRole, CliType};

#[tokio::test]
async fn spawn_and_list() {
    let sup = AgentSupervisor::new();

    let id = sup
        .spawn_agent("mayor-1", AgentRole::Mayor, CliType::Claude)
        .await
        .unwrap();

    let agents = sup.list_agents().await;
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0].id, id);
    assert_eq!(agents[0].name, "mayor-1");
    assert_eq!(agents[0].role, AgentRole::Mayor);
    assert_eq!(agents[0].state, AgentState::Active);
}

#[tokio::test]
async fn stop_agent() {
    let sup = AgentSupervisor::new();

    let id = sup
        .spawn_agent("crew-1", AgentRole::Crew, CliType::Codex)
        .await
        .unwrap();

    sup.stop_agent(id).await.unwrap();

    let agents = sup.list_agents().await;
    assert_eq!(agents[0].state, AgentState::Stopped);
}

#[tokio::test]
async fn heartbeat_all() {
    let sup = AgentSupervisor::new();

    sup.spawn_agent("deacon-1", AgentRole::Deacon, CliType::Claude)
        .await
        .unwrap();
    sup.spawn_agent("witness-1", AgentRole::Witness, CliType::Gemini)
        .await
        .unwrap();

    // Should not error — both agents are Active.
    sup.send_heartbeat_all().await.unwrap();
}

#[tokio::test]
async fn stop_nonexistent_agent_errors() {
    let sup = AgentSupervisor::new();
    let fake_id = uuid::Uuid::new_v4();
    let result = sup.stop_agent(fake_id).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn spawn_all_roles() {
    let sup = AgentSupervisor::new();

    sup.spawn_agent("m", AgentRole::Mayor, CliType::Claude)
        .await
        .unwrap();
    sup.spawn_agent("d", AgentRole::Deacon, CliType::Claude)
        .await
        .unwrap();
    sup.spawn_agent("w", AgentRole::Witness, CliType::Gemini)
        .await
        .unwrap();
    sup.spawn_agent("r", AgentRole::Refinery, CliType::Codex)
        .await
        .unwrap();
    sup.spawn_agent("p", AgentRole::Polecat, CliType::OpenCode)
        .await
        .unwrap();
    sup.spawn_agent("c", AgentRole::Crew, CliType::Claude)
        .await
        .unwrap();

    assert_eq!(sup.agent_count().await, 6);
}
