use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, Error as SqlError, ErrorCode, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: i64,
    pub name: String,
    pub archived_at_unix_ms: Option<i64>,
    pub created_at_unix_ms: i64,
    pub updated_at_unix_ms: i64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectInput {
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProjectInput {
    pub name: String,
}

fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

fn database_error(_: SqlError) -> String {
    "database operation failed".to_string()
}

fn validate_id(id: i64) -> Result<(), String> {
    if id > 0 {
        Ok(())
    } else {
        Err("project id must be greater than 0".to_string())
    }
}

fn validate_name(name: &str) -> Result<String, String> {
    let name = name.trim().to_string();
    if !(1..=100).contains(&name.chars().count()) {
        return Err("project name must be between 1 and 100 characters".to_string());
    }
    Ok(name)
}

fn project_from_row(row: &Row<'_>) -> rusqlite::Result<Project> {
    Ok(Project {
        id: row.get(0)?,
        name: row.get(1)?,
        archived_at_unix_ms: row.get(2)?,
        created_at_unix_ms: row.get(3)?,
        updated_at_unix_ms: row.get(4)?,
    })
}

fn get_project(connection: &Connection, id: i64) -> Result<Project, String> {
    connection
        .query_row(
            "
            SELECT id, name, archived_at_unix_ms, created_at_unix_ms, updated_at_unix_ms
            FROM projects
            WHERE id = ?1
            ",
            [id],
            project_from_row,
        )
        .optional()
        .map_err(database_error)?
        .ok_or_else(|| "project not found".to_string())
}

fn active_project_name_conflict(error: &SqlError) -> bool {
    matches!(
        error,
        SqlError::SqliteFailure(code, message)
            if code.code == ErrorCode::ConstraintViolation
                && message.as_deref().is_some_and(|message| {
                    message.contains("projects.name")
                        || message.contains("idx_projects_active_name")
                })
    )
}

fn write_error(error: SqlError) -> String {
    if active_project_name_conflict(&error) {
        "an active project with this name already exists".to_string()
    } else {
        database_error(error)
    }
}

pub fn list(connection: &Connection, include_archived: bool) -> Result<Vec<Project>, String> {
    let mut statement = connection
        .prepare(
            "
            SELECT id, name, archived_at_unix_ms, created_at_unix_ms, updated_at_unix_ms
            FROM projects
            WHERE ?1 OR archived_at_unix_ms IS NULL
            ORDER BY
                archived_at_unix_ms IS NOT NULL ASC,
                updated_at_unix_ms DESC,
                id DESC
            ",
        )
        .map_err(database_error)?;
    let projects = statement
        .query_map([include_archived], project_from_row)
        .map_err(database_error)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(database_error)?;
    Ok(projects)
}

pub fn create(connection: &mut Connection, input: &CreateProjectInput) -> Result<Project, String> {
    let name = validate_name(&input.name)?;
    let transaction = connection.transaction().map_err(database_error)?;
    let now = now_unix_ms();
    transaction
        .execute(
            "
            INSERT INTO projects (name, archived_at_unix_ms, created_at_unix_ms, updated_at_unix_ms)
            VALUES (?1, NULL, ?2, ?2)
            ",
            params![name, now],
        )
        .map_err(write_error)?;
    let project = get_project(&transaction, transaction.last_insert_rowid())?;
    transaction.commit().map_err(database_error)?;
    Ok(project)
}

pub fn update(
    connection: &mut Connection,
    id: i64,
    input: &UpdateProjectInput,
) -> Result<Project, String> {
    validate_id(id)?;
    let name = validate_name(&input.name)?;
    let transaction = connection.transaction().map_err(database_error)?;
    get_project(&transaction, id)?;
    transaction
        .execute(
            "UPDATE projects SET name = ?1, updated_at_unix_ms = ?2 WHERE id = ?3",
            params![name, now_unix_ms(), id],
        )
        .map_err(write_error)?;
    let project = get_project(&transaction, id)?;
    transaction.commit().map_err(database_error)?;
    Ok(project)
}

pub fn archive(connection: &mut Connection, id: i64) -> Result<Project, String> {
    validate_id(id)?;
    let transaction = connection.transaction().map_err(database_error)?;
    let project = get_project(&transaction, id)?;
    if project.archived_at_unix_ms.is_some() {
        return Err("project is already archived".to_string());
    }
    let has_active_tasks: bool = transaction
        .query_row(
            "
            SELECT EXISTS(
                SELECT 1
                FROM tasks
                WHERE project_id = ?1
                  AND completed_at_unix_ms IS NULL
                  AND deleted_at_unix_ms IS NULL
            )
            ",
            [id],
            |row| row.get(0),
        )
        .map_err(database_error)?;
    if has_active_tasks {
        return Err("cannot archive a project with unfinished tasks".to_string());
    }
    let now = now_unix_ms();
    transaction
        .execute(
            "
            UPDATE projects
            SET archived_at_unix_ms = ?1, updated_at_unix_ms = ?1
            WHERE id = ?2
            ",
            params![now, id],
        )
        .map_err(database_error)?;
    let project = get_project(&transaction, id)?;
    transaction.commit().map_err(database_error)?;
    Ok(project)
}

pub fn restore(connection: &mut Connection, id: i64) -> Result<Project, String> {
    validate_id(id)?;
    let transaction = connection.transaction().map_err(database_error)?;
    let project = get_project(&transaction, id)?;
    if project.archived_at_unix_ms.is_none() {
        return Err("project is not archived".to_string());
    }
    transaction
        .execute(
            "
            UPDATE projects
            SET archived_at_unix_ms = NULL, updated_at_unix_ms = ?1
            WHERE id = ?2
            ",
            params![now_unix_ms(), id],
        )
        .map_err(write_error)?;
    let project = get_project(&transaction, id)?;
    transaction.commit().map_err(database_error)?;
    Ok(project)
}

pub fn validate_project_assignment(
    connection: &Connection,
    project_id: Option<i64>,
) -> Result<(), String> {
    let Some(project_id) = project_id else {
        return Ok(());
    };
    validate_id(project_id)?;
    let archived_at_unix_ms = connection
        .query_row(
            "SELECT archived_at_unix_ms FROM projects WHERE id = ?1",
            [project_id],
            |row| row.get::<_, Option<i64>>(0),
        )
        .optional()
        .map_err(database_error)?;
    match archived_at_unix_ms {
        None => Err("project not found".to_string()),
        Some(Some(_)) => Err("project is archived".to_string()),
        Some(None) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connection() -> Connection {
        let connection = Connection::open_in_memory().expect("in-memory database should open");
        connection
            .execute_batch(
                "
                PRAGMA foreign_keys = ON;
                CREATE TABLE projects (
                    id INTEGER PRIMARY KEY,
                    name TEXT NOT NULL,
                    archived_at_unix_ms INTEGER,
                    created_at_unix_ms INTEGER NOT NULL,
                    updated_at_unix_ms INTEGER NOT NULL
                );
                CREATE UNIQUE INDEX idx_projects_active_name
                    ON projects (name COLLATE NOCASE)
                    WHERE archived_at_unix_ms IS NULL;
                CREATE TABLE tasks (
                    id INTEGER PRIMARY KEY,
                    project_id INTEGER REFERENCES projects(id),
                    completed_at_unix_ms INTEGER,
                    deleted_at_unix_ms INTEGER
                );
                ",
            )
            .expect("test schema should create");
        connection
    }

    fn create_input(name: &str) -> CreateProjectInput {
        CreateProjectInput {
            name: name.to_string(),
        }
    }

    #[test]
    fn crud_trims_names_and_lists_active_projects_first() {
        let mut connection = connection();
        let first =
            create(&mut connection, &create_input("  First  ")).expect("project should create");
        let second =
            create(&mut connection, &create_input("Second")).expect("project should create");
        assert_eq!(
            create(&mut connection, &create_input("   ")).unwrap_err(),
            "project name must be between 1 and 100 characters"
        );
        assert_eq!(
            create(&mut connection, &create_input(&"字".repeat(101))).unwrap_err(),
            "project name must be between 1 and 100 characters"
        );
        let updated = update(
            &mut connection,
            first.id,
            &UpdateProjectInput {
                name: "Renamed".to_string(),
            },
        )
        .expect("project should update");
        assert_eq!(updated.name, "Renamed");

        let archived = archive(&mut connection, second.id).expect("project should archive");
        assert!(archived.archived_at_unix_ms.is_some());
        assert_eq!(
            list(&connection, false)
                .expect("active projects should list")
                .into_iter()
                .map(|project| project.id)
                .collect::<Vec<_>>(),
            vec![first.id]
        );
        assert_eq!(
            list(&connection, true)
                .expect("all projects should list")
                .into_iter()
                .map(|project| project.id)
                .collect::<Vec<_>>(),
            vec![first.id, second.id]
        );
    }

    #[test]
    fn active_project_names_are_unique_case_insensitively() {
        let mut connection = connection();
        let created =
            create(&mut connection, &create_input("Roadmap")).expect("project should create");
        assert_eq!(
            create(&mut connection, &create_input("roadmap")).unwrap_err(),
            "an active project with this name already exists"
        );

        archive(&mut connection, created.id).expect("project should archive");
        create(&mut connection, &create_input("roadmap"))
            .expect("an archived name should be reusable");
    }

    #[test]
    fn restore_rejects_an_active_name_conflict_without_changing_archived_state() {
        let mut connection = connection();
        let archived =
            create(&mut connection, &create_input("Roadmap")).expect("project should create");
        archive(&mut connection, archived.id).expect("project should archive");
        create(&mut connection, &create_input("roadmap"))
            .expect("active project should reuse an archived name");

        assert_eq!(
            restore(&mut connection, archived.id).unwrap_err(),
            "an active project with this name already exists"
        );
        assert!(get_project(&connection, archived.id)
            .expect("archived project should still load")
            .archived_at_unix_ms
            .is_some());
    }

    #[test]
    fn archive_requires_every_non_deleted_task_to_be_completed() {
        let mut connection = connection();
        let project =
            create(&mut connection, &create_input("Work")).expect("project should create");
        connection
            .execute(
                "INSERT INTO tasks (project_id, completed_at_unix_ms, deleted_at_unix_ms) VALUES (?1, NULL, NULL)",
                [project.id],
            )
            .expect("unfinished task should insert");
        assert_eq!(
            archive(&mut connection, project.id).unwrap_err(),
            "cannot archive a project with unfinished tasks"
        );

        connection
            .execute(
                "UPDATE tasks SET completed_at_unix_ms = 1 WHERE project_id = ?1",
                [project.id],
            )
            .expect("task should complete");
        archive(&mut connection, project.id).expect("completed project should archive");
    }

    #[test]
    fn restore_only_accepts_archived_projects() {
        let mut connection = connection();
        let project =
            create(&mut connection, &create_input("Personal")).expect("project should create");
        assert_eq!(
            restore(&mut connection, project.id).unwrap_err(),
            "project is not archived"
        );
        archive(&mut connection, project.id).expect("project should archive");
        let restored = restore(&mut connection, project.id).expect("project should restore");
        assert_eq!(restored.archived_at_unix_ms, None);
    }

    #[test]
    fn project_assignment_requires_an_existing_active_foreign_key_target() {
        let mut connection = connection();
        let project =
            create(&mut connection, &create_input("Client")).expect("project should create");
        validate_project_assignment(&connection, None).expect("no project is valid");
        validate_project_assignment(&connection, Some(project.id))
            .expect("active project is valid");
        assert_eq!(
            validate_project_assignment(&connection, Some(9_999)).unwrap_err(),
            "project not found"
        );
        assert!(connection
            .execute("INSERT INTO tasks (project_id) VALUES (9_999)", [])
            .is_err());

        archive(&mut connection, project.id).expect("project should archive");
        assert_eq!(
            validate_project_assignment(&connection, Some(project.id)).unwrap_err(),
            "project is archived"
        );
        connection
            .execute("INSERT INTO tasks (project_id) VALUES (NULL)", [])
            .expect("a task may omit its project");
    }
}
