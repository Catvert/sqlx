use sqlx::MySql;
use sqlx_test::new;

#[cfg_attr(feature = "runtime-async-std", async_std::test)]
#[cfg_attr(feature = "runtime-tokio", tokio::test)]
async fn macro_select_from_cte() -> anyhow::Result<()> {
    let mut conn = new::<MySql>().await?;
    let account =
        sqlx::query!("select * from (select (1) as id, 'Herp Derpinson' as name) accounts")
            .fetch_one(&mut conn)
            .await?;

    println!("{:?}", account);
    println!("{}: {}", account.id, account.name);

    Ok(())
}

#[cfg_attr(feature = "runtime-async-std", async_std::test)]
#[cfg_attr(feature = "runtime-tokio", tokio::test)]
async fn macro_select_from_cte_bind() -> anyhow::Result<()> {
    let mut conn = new::<MySql>().await?;
    let account = sqlx::query!(
        "select * from (select (1) as id, 'Herp Derpinson' as name) accounts where id = ?",
        1i32
    )
    .fetch_one(&mut conn)
    .await?;

    println!("{:?}", account);
    println!("{}: {}", account.id, account.name);

    Ok(())
}

#[derive(Debug)]
struct RawAccount {
    r#type: i32,
    name: Option<String>,
}

#[cfg_attr(feature = "runtime-async-std", async_std::test)]
#[cfg_attr(feature = "runtime-tokio", tokio::test)]
async fn test_query_as_raw() -> anyhow::Result<()> {
    let mut conn = new::<MySql>().await?;

    let account = sqlx::query_as!(
        RawAccount,
        "SELECT * from (select 1 as type, cast(null as char) as name) accounts"
    )
    .fetch_one(&mut conn)
    .await?;

    assert_eq!(None, account.name);
    assert_eq!(1, account.r#type);

    println!("{:?}", account);

    Ok(())
}
