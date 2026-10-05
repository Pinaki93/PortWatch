use loco_rs::testing::prelude::*;
use port_watch::app::App;

#[tokio::test]
async fn serves_dashboard_and_api() {
    request::<App, _, _>(|request, _ctx| async move {
        let page = request.get("/").await;
        assert_eq!(page.status_code(), 200);
        assert!(page.text().contains("PORT / WATCH"));
        assert!(page.text().contains("Custom open command"));

        let api = request.get("/api").await;
        assert_eq!(api.status_code(), 200);
        let body = api.json::<serde_json::Value>();
        assert_eq!(body["name"], "Port Watch");
        assert!(body["endpoints"]
            .as_array()
            .is_some_and(|endpoints| endpoints.contains(&"POST /api/pins".into())));
    })
    .await;
}
