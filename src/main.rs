// main.rs
use poem::{middleware::Cors, EndpointExt, Route, listener::TcpListener};
use poem_openapi::{OpenApi, OpenApiService, payload::Json, param::{Path, Query}, Object};
use sea_orm::{Database, DatabaseConnection, EntityTrait, ActiveModelTrait, Set};
use std::sync::Arc;

mod entities; // mod posts 등

#[derive(Clone)]
struct AppState {
    db: DatabaseConnection,
}

#[derive(Object)]
struct PostCreate { title: String, body: String }
#[derive(Object)]
struct PostUpdate { title: Option<String>, body: Option<String> }

#[derive(Default)]
struct Api {
    state: Arc<AppState>,
}

#[OpenApi]
impl Api {
    /// 목록 (페이지네이션)
    #[oai(path = "/posts", method = "get")]
    async fn list_posts(&self, Query(page): Query<Option<u64>>, Query(per_page): Query<Option<u64>>) 
        -> poem::Result<Json<Vec<entities::post::Model>>> 
    {
        use entities::post::Entity as Post;
        let page = page.unwrap_or(1).max(1) - 1;
        let per = per_page.unwrap_or(20).clamp(1, 100);
        let paginator = Post::find().order_by_desc(entities::post::Column::IsPinned)
                                   .order_by_desc(entities::post::Column::Id)
                                   .paginate(&self.state.db, per);
        let rows = paginator.fetch_page(page).await.map_err(anyhow_to_poem)?;
        Ok(Json(rows))
    }

    /// 단건 조회
    #[oai(path = "/posts/:id", method = "get")]
    async fn get_post(&self, Path(id): Path<i64>) -> poem::Result<Json<entities::post::Model>> {
        use entities::post::Entity as Post;
        let post = Post::find_by_id(id).one(&self.state.db).await.map_err(anyhow_to_poem)?;
        post.map(Json).ok_or_else(|| not_found("post"))
    }

    /// 생성
    #[oai(path = "/posts", method = "post")]
    async fn create_post(&self, Json(input): Json<PostCreate>) -> poem::Result<Json<entities::post::Model>> {
        use entities::post::{ActiveModel, Entity as Post};
        let mut am = ActiveModel {
            title: Set(input.title),
            body: Set(input.body),
            ..Default::default()
        };
        let created = am.insert(&self.state.db).await.map_err(anyhow_to_poem)?;
        Ok(Json(created))
    }

    /// 수정
    #[oai(path = "/posts/:id", method = "put")]
    async fn update_post(&self, Path(id): Path<i64>, Json(input): Json<PostUpdate>) 
        -> poem::Result<Json<entities::post::Model>> 
    {
        use entities::post::{ActiveModel, Entity as Post};
        let found = Post::find_by_id(id).one(&self.state.db).await.map_err(anyhow_to_poem)?;
        let mut am: ActiveModel = found.ok_or_else(|| not_found("post"))?.into();
        if let Some(t) = input.title { am.title = Set(t); }
        if let Some(b) = input.body { am.body = Set(b); }
        let updated = am.update(&self.state.db).await.map_err(anyhow_to_poem)?;
        Ok(Json(updated))
    }

    /// 삭제
    #[oai(path = "/posts/:id", method = "delete")]
    async fn delete_post(&self, Path(id): Path<i64>) -> poem::Result<()> {
        use entities::post::Entity as Post;
        let res = Post::delete_by_id(id).exec(&self.state.db).await.map_err(anyhow_to_poem)?;
        if res.rows_affected == 0 { return Err(not_found("post")); }
        Ok(())
    }
}

fn not_found(what: &str) -> poem::Error {
    poem::Error::from_string(format!("{what} not found"), poem::http::StatusCode::NOT_FOUND)
}
fn anyhow_to_poem(e: impl std::fmt::Display) -> poem::Error {
    poem::Error::from_string(e.to_string(), poem::http::StatusCode::INTERNAL_SERVER_ERROR)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt().with_env_filter("info").init();

    let db = Database::connect(std::env::var("DATABASE_URL")?).await?;
    let state = Arc::new(AppState { db });

    let api = OpenApiService::new(Api { state: state.clone() }, "Board API", "1.0")
        .server("/api");
    let ui = api.swagger_ui();

    let app = Route::new()
        .nest("/api", api)
        .nest("/", ui)
        .with(Cors::new()); // 필요 시 설정 강화

    poem::Server::new(TcpListener::bind("0.0.0.0:3000"))
        .run(app)
        .await?;
    Ok(())
}
