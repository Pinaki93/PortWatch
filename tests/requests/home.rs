use loco_rs::testing::prelude::*;
use port_watch::app::App;

#[tokio::test]
async fn serves_dashboard_and_api() {
    request::<App, _, _>(|request, _ctx| async move {
        let page = request.get("/").await;
        assert_eq!(page.status_code(), 200);
        assert!(page.text().contains("PORT / WATCH"));

        let api = request.get("/api").await;
        assert_eq!(api.status_code(), 200);
        assert_eq!(api.json::<serde_json::Value>()["name"], "Port Watch");
    })
    .await;
}
