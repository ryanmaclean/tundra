use at_core::cache::CacheDb;
use at_core::types::*;

#[tokio::test]
async fn bead_upsert_and_get() {
    let db = CacheDb::new_in_memory().await.unwrap();
    let bead = Bead::new("cache test", Lane::Standard);
    let id = bead.id;

    db.upsert_bead(&bead).await.unwrap();

    let fetched = db.get_bead(id).await.unwrap().expect("bead should exist");
    assert_eq!(fetched.title, "cache test");
    assert_eq!(fetched.lane, Lane::Standard);
    assert_eq!(fetched.status, BeadStatus::Backlog);
}

#[tokio::test]
async fn bead_update() {
    let db = CacheDb::new_in_memory().await.unwrap();
    let mut bead = Bead::new("original", Lane::Experimental);
    let id = bead.id;

    db.upsert_bead(&bead).await.unwrap();

    bead.title = "updated".to_string();
    bead.status = BeadStatus::Hooked;
    db.upsert_bead(&bead).await.unwrap();

    let fetched = db.get_bead(id).await.unwrap().unwrap();
    assert_eq!(fetched.title, "updated");
    assert_eq!(fetched.status, BeadStatus::Hooked);
}

#[tokio::test]
async fn list_beads_by_status() {
    let db = CacheDb::new_in_memory().await.unwrap();

    let b1 = Bead::new("a", Lane::Standard);
    let b2 = Bead::new("b", Lane::Critical);
    let mut b3 = Bead::new("c", Lane::Experimental);
    b3.status = BeadStatus::Hooked;

    db.upsert_bead(&b1).await.unwrap();
    db.upsert_bead(&b2).await.unwrap();
    db.upsert_bead(&b3).await.unwrap();

    let backlog = db.list_beads_by_status(BeadStatus::Backlog).await.unwrap();
    assert_eq!(backlog.len(), 2);

    let hooked = db.list_beads_by_status(BeadStatus::Hooked).await.unwrap();
    assert_eq!(hooked.len(), 1);
    assert_eq!(hooked[0].title, "c");
}

#[tokio::test]
async fn agent_upsert_and_get_by_name() {
    let db = CacheDb::new_in_memory().await.unwrap();
    let agent = Agent::new("deacon-1", AgentRole::Deacon, CliType::Codex);

    db.upsert_agent(&agent).await.unwrap();

    let fetched = db
        .get_agent_by_name("deacon-1")
        .await
        .unwrap()
        .expect("agent should exist");
    assert_eq!(fetched.name, "deacon-1");
    assert_eq!(fetched.role, AgentRole::Deacon);
    assert_eq!(fetched.cli_type, CliType::Codex);
    assert_eq!(fetched.status, AgentStatus::Pending);
}

#[tokio::test]
async fn agent_not_found() {
    let db = CacheDb::new_in_memory().await.unwrap();
    let result = db.get_agent_by_name("nonexistent").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn kpi_snapshot() {
    let db = CacheDb::new_in_memory().await.unwrap();

    let b1 = Bead::new("task1", Lane::Standard);
    let mut b2 = Bead::new("task2", Lane::Critical);
    b2.status = BeadStatus::Done;
    let mut b3 = Bead::new("task3", Lane::Experimental);
    b3.status = BeadStatus::Failed;

    db.upsert_bead(&b1).await.unwrap();
    db.upsert_bead(&b2).await.unwrap();
    db.upsert_bead(&b3).await.unwrap();

    let mut agent = Agent::new("kpi-agent", AgentRole::Crew, CliType::Claude);
    agent.status = AgentStatus::Active;
    db.upsert_agent(&agent).await.unwrap();

    let kpi = db.compute_kpi_snapshot().await.unwrap();
    assert_eq!(kpi.total_beads, 3);
    assert_eq!(kpi.backlog, 1);
    assert_eq!(kpi.done, 1);
    assert_eq!(kpi.failed, 1);
    assert_eq!(kpi.active_agents, 1);
}
