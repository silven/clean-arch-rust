use crate::domain::entities::{User, Task};

use rusqlite::{Connection, NO_PARAMS};

// I hate that this needs to be public for the binary to use it
pub trait SQLable {
    type Inner;

    fn select_one() -> &'static str;
    fn select_all() -> &'static str;
    fn insert_one() -> &'static str;
    fn create_table() -> &'static str;

    // I hate this allocation.
    fn bind(data: &Self::Inner) -> Vec<&rusqlite::types::ToSql>;

    fn from_row<'row, 'stmt>(row: &rusqlite::Row<'row, 'stmt>) -> Self::Inner;
}

// Some kind of generalization so I can extract the things that differ.
// The major drawback I found with this, is the problems related to the relation
// between different objects. A User has Tasks, but I don't get them like this
// and a LEFT JOIN doesn't really help, because then we need to post-process the data
impl SQLable for User {
    type Inner = User;

    fn select_one() -> &'static str { "SELECT `name` FROM users WHERE id = (?) LIMIT 1" }
    fn select_all() -> &'static str { "SELECT `name` FROM users" }
    fn insert_one() -> &'static str { "INSERT INTO users (name) VALUES (?)" }
    fn create_table() -> &'static str { "CREATE TABLE users (
                id         INTEGER PRIMARY KEY,
                name       TEXT NOT NULL
            )"
        }

    fn bind(user: &User) -> Vec<&rusqlite::types::ToSql> { vec![&user.name] }

    fn from_row<'row, 'stmt>(row: &rusqlite::Row<'row, 'stmt>) -> Self::Inner {
        let name: String = row.get(0);
        User::new(name)
    }
}

impl SQLable for Task {
    type Inner = Task;

    fn select_one() -> &'static str { "SELECT desc from tasks WHERE id = (?) LIMIT 1" }
    fn select_all() -> &'static str { "SELECT desc from tasks" }
    fn insert_one() -> &'static str { "INSERT INTO tasks (desc) VALUES (?)" }
    fn create_table() -> &'static str { "CREATE TABLE tasks (
                id         INTEGER PRIMARY KEY,
                desc       TEXT NOT NULL
            )"
        }

    fn bind(task: &Task) -> Vec<&rusqlite::types::ToSql> { vec![&task.desc] }

    fn from_row<'row, 'stmt>(row: &rusqlite::Row<'row, 'stmt>) -> Self::Inner {
        let desc: String = row.get(0);
        Task::new(desc)
    }
}

// The thing that ties this imlpementation to rusqlite
use std::marker::PhantomData;
pub struct Rusqlite<T: SQLable> {
    conn: rusqlite::Connection,
    _phantom: PhantomData<T>,
}


impl<T: SQLable> Rusqlite<T> {
    pub fn in_memory() -> Self {
        Rusqlite {
            conn: Connection::open_in_memory().expect("Could not open Database"),
            _phantom: PhantomData,
        }
    }

    pub fn setup(&self) -> Result<usize, rusqlite::Error> {
        self.conn.execute(T::create_table(), NO_PARAMS)
    }
}

use crate::domain::Repository;
// Blanket implementation for all things SQLable
impl<T: SQLable> Repository<T::Inner> for Rusqlite<T> {
    type Id = i64;

    fn all(&self) -> Vec<T::Inner> {
        let mut stmnt = self.conn
            .prepare(T::select_all())
            .expect("Could not prepare all statement");

        let iter = stmnt.query_map(NO_PARAMS, |row| T::from_row(row))
            .expect("Could not bind params");

        let mut result = Vec::new();
        for elem in iter {
            result.push(elem.expect("Could not construct"));
        }
        result
    }

    fn find(&self, id: &i64) -> Option<T::Inner> {
        let mut stmnt = self.conn
            .prepare(T::select_one())
            .expect("Could not prepare statement");

        let mut iter = stmnt.query_map(&[id], |row| T::from_row(row))
            .expect("Could not bind params");

        while let Some(Ok(elem)) = iter.next() {
            return Some(elem);
        }

        None
    }

    fn save(&mut self, data: &T::Inner) -> Self::Id {
        let mut stmnt = self.conn.prepare(T::insert_one()).expect("Could not prepare save");
        stmnt.insert(T::bind(data)).expect("Could not insert")
    }

}

// Two different In Memory Repository implementations, just to prove the concept.
pub struct TrivialRepository<T: Clone>(Vec<T>);

impl<T: Clone> TrivialRepository<T> {
    pub fn new() -> Self {
        TrivialRepository(Vec::new())
    }
}

impl<T: Clone> Repository<T> for TrivialRepository<T> {
    type Id = usize;

    fn all(&self) -> Vec<T> {
        self.0.clone()
    }

    fn find(&self, id: &Self::Id) -> Option<T> {
        self.0.get(*id).cloned()
    }

    fn save(&mut self, data: &T) -> Self::Id {
        let idx = self.0.len();
        self.0.push(data.clone());
        idx
    }

}

use std::collections::HashMap;
use uuid::Uuid;

pub struct HashRepository<T: Clone>(HashMap<Uuid, T>);

impl<T: Clone> HashRepository<T> {
    pub fn new() -> Self {
        HashRepository(HashMap::new())
    }
}

impl<T: Clone> Repository<T> for HashRepository<T> {
    type Id = Uuid;

    fn all(&self) -> Vec<T> {
        self.0.values().cloned().collect()
    }

    fn find(&self, id: &Self::Id) -> Option<T> {
        self.0.get(id).cloned()
    }

    fn save(&mut self, data: &T) -> Self::Id {
        let id = Uuid::new_v4();
        self.0.insert(id, data.clone());
        id
    }

}

#[cfg(test)]
mod test {
    use crate::domain::Repository;

    fn test_save_and_get<R: Repository<String>>(mut repo: R) {
        let input = "This is a cool string".to_string();
        let id = repo.save(&input);
        let output = repo.find(&id).expect("Could not find what I just put in!");
        assert_eq!(input, output);
    }

    #[test]
    fn hash_repo_works() {
        let hash_repo = super::HashRepository::new();
        test_save_and_get(hash_repo);
    }

    #[test]
    fn vec_repo_works() {
        let vec_repo = super::TrivialRepository::new();
        test_save_and_get(vec_repo);
    }


    #[test]
    fn test_get_all_done_via_id() {
        use crate::domain::{entities::{User, Task}, Repository};

        let mut one_done = Task::new("One");
        one_done.finish();
        let mut two_done = Task::new("Two");
        two_done.finish();
        let not_done = Task::new("Tre");

        let mut user: User = User::new("Someone");

        user.add_task(one_done.clone());
        user.add_task(not_done.clone());
        user.add_task(two_done.clone());

        let mut repo = crate::data::HashRepository::new();
        let user_id = repo.save(&user);

        let found_done = crate::domain::usecases::find_all_done_via_id(&repo, &user_id);

        assert_eq!(found_done, vec![one_done, two_done]);
    }
}