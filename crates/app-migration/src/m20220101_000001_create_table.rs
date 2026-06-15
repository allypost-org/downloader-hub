use sea_orm_migration::{
    prelude::*,
    sea_orm::{ActiveEnum, DeriveActiveEnum, EnumIter, Schema},
    sea_query::extension::postgres::Type,
};

use crate::common::{GenKeyType, generate_index};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        {
            let db = manager.get_connection();

            let stmt = r#"
                CREATE COLLATION if not exists "ignore_accent_case" (provider = icu, locale = 'und-u-ks-level1', deterministic = false);
            "#.trim();

            debug_print!(stmt);

            db.execute_unprepared(stmt).await?;
        }

        {
            let stmt = Table::create()
                .table(Client::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(Client::Id)
                        .integer()
                        .not_null()
                        .auto_increment()
                        .primary_key(),
                )
                .col(
                    ColumnDef::new(Client::Name)
                        .text()
                        .extra("COLLATE \"ignore_accent_case\"")
                        .unique_key()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(Client::ApiKey)
                        .text()
                        .not_null()
                        .unique_key(),
                )
                .col(
                    ColumnDef::new(Client::AppMeta)
                        .json_binary()
                        .not_null()
                        .default(Expr::val("{}")),
                )
                .col(
                    ColumnDef::new(Client::DownloadFolder)
                        .json_binary()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(Client::CreatedAt)
                        .timestamp_with_time_zone()
                        .not_null()
                        .default(Expr::current_timestamp()),
                )
                .col(
                    ColumnDef::new(Client::UpdatedAt)
                        .timestamp_with_time_zone()
                        .not_null()
                        .default(Expr::current_timestamp()),
                )
                .to_owned();
            debug_print!(stmt.to_string(PostgresQueryBuilder));
            manager.create_table(stmt).await?;

            let stmt = generate_index(Client::Table, vec![Client::ApiKey]);
            debug_print!(stmt.to_string(PostgresQueryBuilder));
            manager.create_index(stmt).await?;
        }

        {
            let stmt = Schema::new(manager.get_database_backend())
                .create_enum_from_active_enum::<ItemStatus>()
                .to_owned();
            debug_print!(stmt.to_string(PostgresQueryBuilder));
            manager.create_type(stmt).await?;
        }

        {
            let stmt = Table::create()
                .table(DownloadRequest::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(DownloadRequest::Id)
                        .integer()
                        .not_null()
                        .auto_increment()
                        .primary_key(),
                )
                .col(
                    ColumnDef::new(DownloadRequest::RequestUid)
                        .text()
                        .not_null()
                        .unique_key(),
                )
                .col(
                    ColumnDef::new(DownloadRequest::ClientId)
                        .integer()
                        .not_null(),
                )
                .col(ColumnDef::new(DownloadRequest::Url).text().not_null())
                .col(
                    ColumnDef::new(DownloadRequest::Status)
                        .custom(ItemStatusEnum)
                        .not_null()
                        .default(ItemStatus::Pending),
                )
                .col(
                    ColumnDef::new(DownloadRequest::Meta)
                        .json_binary()
                        .not_null()
                        .default(Expr::val("{}")),
                )
                .col(
                    ColumnDef::new(DownloadRequest::AppMeta)
                        .json_binary()
                        .not_null()
                        .default(Expr::val("{}")),
                )
                .col(
                    ColumnDef::new(DownloadRequest::CreatedAt)
                        .timestamp_with_time_zone()
                        .not_null()
                        .default(Expr::current_timestamp()),
                )
                .col(
                    ColumnDef::new(DownloadRequest::UpdatedAt)
                        .timestamp_with_time_zone()
                        .not_null()
                        .default(Expr::current_timestamp()),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name(GenKeyType::ForeignKey.gen_name(
                            &DownloadRequest::Table.to_string(),
                            DownloadRequest::ClientId,
                        ))
                        .from(DownloadRequest::Table, DownloadRequest::ClientId)
                        .to(Client::Table, Client::Id)
                        .on_delete(ForeignKeyAction::Cascade)
                        .on_update(ForeignKeyAction::Cascade),
                )
                .to_owned();
            debug_print!(stmt.to_string(PostgresQueryBuilder));
            manager.create_table(stmt).await?;

            let stmt = generate_index(
                DownloadRequest::Table,
                vec![(DownloadRequest::RequestUid, IndexOrder::Desc)],
            );
            debug_print!(stmt.to_string(PostgresQueryBuilder));
            manager.create_index(stmt).await?;

            let stmt = generate_index(DownloadRequest::Table, vec![DownloadRequest::Status]);
            debug_print!(stmt.to_string(PostgresQueryBuilder));
            manager.create_index(stmt).await?;

            let stmt = generate_index(DownloadRequest::Table, vec![DownloadRequest::ClientId]);
            debug_print!(stmt.to_string(PostgresQueryBuilder));
            manager.create_index(stmt).await?;
        }

        {
            let stmt = Table::create()
                .table(DownloadResult::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(DownloadResult::Id)
                        .integer()
                        .not_null()
                        .auto_increment()
                        .primary_key(),
                )
                .col(
                    ColumnDef::new(DownloadResult::ResultUid)
                        .text()
                        .not_null()
                        .unique_key(),
                )
                .col(
                    ColumnDef::new(DownloadResult::DownloadRequestId)
                        .integer()
                        .not_null(),
                )
                .col(ColumnDef::new(DownloadResult::Path).json_binary())
                .col(
                    ColumnDef::new(DownloadResult::Status)
                        .custom(ItemStatusEnum)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(DownloadResult::Meta)
                        .json_binary()
                        .not_null()
                        .default(Expr::val("{}")),
                )
                .col(
                    ColumnDef::new(DownloadResult::CreatedAt)
                        .timestamp_with_time_zone()
                        .not_null()
                        .default(Expr::current_timestamp()),
                )
                .col(
                    ColumnDef::new(DownloadResult::UpdatedAt)
                        .timestamp_with_time_zone()
                        .not_null()
                        .default(Expr::current_timestamp()),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name(GenKeyType::ForeignKey.gen_name(
                            &DownloadResult::Table.to_string(),
                            DownloadResult::DownloadRequestId,
                        ))
                        .from(DownloadResult::Table, DownloadResult::DownloadRequestId)
                        .to(DownloadRequest::Table, DownloadRequest::Id)
                        .on_delete(ForeignKeyAction::Cascade)
                        .on_update(ForeignKeyAction::Cascade),
                )
                .to_owned();
            debug_print!(stmt.to_string(PostgresQueryBuilder));
            manager.create_table(stmt).await?;

            let stmt = generate_index(DownloadResult::Table, vec![DownloadResult::ResultUid]);
            debug_print!(stmt.to_string(PostgresQueryBuilder));
            manager.create_index(stmt).await?;

            let stmt = generate_index(
                DownloadResult::Table,
                vec![DownloadResult::DownloadRequestId],
            );
            debug_print!(stmt.to_string(PostgresQueryBuilder));
            manager.create_index(stmt).await?;

            let stmt = generate_index(DownloadResult::Table, vec![DownloadResult::Path]);
            debug_print!(stmt.to_string(PostgresQueryBuilder));
            manager.create_index(stmt).await?;
        }

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(DownloadResult::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(DownloadRequest::Table).to_owned())
            .await?;

        manager
            .drop_table(Table::drop().table(Client::Table).to_owned())
            .await?;

        manager
            .drop_type(Type::drop().if_exists().name(ItemStatus::name()).to_owned())
            .await?;

        {
            let db = manager.get_connection();

            let stmt = r#"
                DROP COLLATION "ignore_accent_case";
            "#
            .trim();

            db.execute_unprepared(stmt).await?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "Enum", enum_name = "item_status")]
pub enum ItemStatus {
    #[sea_orm(string_value = "pending")]
    Pending,
    #[sea_orm(string_value = "processing")]
    Processing,
    #[sea_orm(string_value = "success")]
    Success,
    #[sea_orm(string_value = "failed")]
    Failed,
}

#[derive(DeriveIden)]
pub enum Client {
    Table,
    #[sea_orm(iden = "_id")]
    Id,
    Name,
    #[sea_orm(iden = "_app_meta")]
    AppMeta,
    ApiKey,
    DownloadFolder,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
pub enum DownloadRequest {
    Table,
    #[sea_orm(iden = "_id")]
    Id,
    RequestUid,
    #[sea_orm(iden = "_client_id")]
    ClientId,
    Url,
    Status,
    Meta,
    #[sea_orm(iden = "_app_meta")]
    AppMeta,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
pub enum DownloadResult {
    Table,
    #[sea_orm(iden = "_id")]
    Id,
    ResultUid,
    #[sea_orm(iden = "_download_request_id")]
    DownloadRequestId,
    Path,
    Status,
    Meta,
    CreatedAt,
    UpdatedAt,
}
