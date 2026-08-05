use surrealdb::Surreal;
use surrealdb::engine::local::Mem;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = Surreal::new::<Mem>(()).await?;
    db.use_ns("totem").use_db("spike").await?;
    totem_store_spike::install_toy_dataset(&db).await?;
    println!("{}", totem_store_spike::explain_scoped_knn(&db).await?);
    Ok(())
}
