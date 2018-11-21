use crate::domain::entities::{User, Task};

use rusqlite::{Connection, NO_PARAMS, types::ToSql};

pub trait SQLable {
    fn select_one() -> &'static str;
    fn select_all() -> &'static str;
    fn insert_one() -> &'static str;
    fn create_table() -> &'static str;

    #[inline(always)]
    fn bind<F, T>(data: &Self, consumer: F) -> T where F: FnOnce(&[&rusqlite::types::ToSql]) -> T;

    fn from_row<'row, 'stmt>(row: &rusqlite::Row<'row, 'stmt>) -> Self;
}

pub trait SQLSearchable : Searchable + SQLable {
    fn build_query(creds: &[<Self as Searchable>::Credentials], limit: Option<u32>) -> (String, Vec<&ToSql>);
}

// Some kind of generalization so I can extract the things that differ.
// The major drawback I found with this, is the problems related to the relation
// between different objects. A User has Tasks, but I don't get them like this
// and a LEFT JOIN doesn't really help, because then we need to post-process the data
impl SQLable for User {

    fn select_one() -> &'static str { "SELECT * FROM users WHERE id = (?) LIMIT 1" }
    fn select_all() -> &'static str { "SELECT * FROM users" }
    fn insert_one() -> &'static str { "INSERT INTO users (name) VALUES (?)" }
    fn create_table() -> &'static str { 
        "CREATE TABLE users (
            id         INTEGER PRIMARY KEY,
            name       TEXT NOT NULL
        )"
    }

    fn bind<F, T>(data: &Self, consumer: F) -> T where F: FnOnce(&[&ToSql]) -> T {
        let bindings = [&data.name as &ToSql];
        consumer(&bindings)
    }

    fn from_row<'row, 'stmt>(row: &rusqlite::Row<'row, 'stmt>) -> Self {
        let name: String = row.get(1);
        User::new(name)
    }
}

impl SQLSearchable for User {
    fn build_query(creds: &[<Self as Searchable>::Credentials], limit: Option<u32>) -> (String, Vec<&ToSql>) {
        let mut sql = User::select_all().to_string();

        if creds.len() > 0 {
            sql += " WHERE";
        }

        let mut params: Vec<&ToSql> = Vec::with_capacity(creds.len());
        let mut iter = creds.iter().peekable();

        while let Some(pred) = iter.next() {
            match pred {
                UserSearchTerms::Name(ref name) => {
                    sql += " name = (?)";
                    params.push(name);
                }
            }
            if iter.peek().is_some() {
                sql += " AND";
            }
        }

        if let Some(limit) = limit {
            sql += &format!(" LIMIT {}", limit);
        }

        (sql, params)
    }
}

impl SQLable for Task {
    fn select_one() -> &'static str { "SELECT * from tasks WHERE id = (?) LIMIT 1" }
    fn select_all() -> &'static str { "SELECT * from tasks" }
    fn insert_one() -> &'static str { "INSERT INTO tasks (desc) VALUES (?)" }
    fn create_table() -> &'static str { 
        "CREATE TABLE tasks (
            id         INTEGER PRIMARY KEY,
            desc       TEXT NOT NULL
        )"
    }

    fn bind<F, T>(data: &Self, consumer: F) -> T where F: FnOnce(&[&ToSql]) -> T {
        let bindings = [&data.desc as &ToSql];
        consumer(&bindings)
    }
    
    fn from_row<'row, 'stmt>(row: &rusqlite::Row<'row, 'stmt>) -> Self {
        let desc: String = row.get(1);
        Task::new(desc)
    }
}

// The thing that ties this imlpementation to rusqlite
pub struct Rusqlite {
    conn: rusqlite::Connection,
}


use crate::domain::entities::UserSearchTerms;
impl Rusqlite {
    pub fn in_memory() -> Self {
        Rusqlite {
            conn: Connection::open_in_memory().expect("Could not open Database"),
        }
    }

    pub fn setup<T: SQLable>(&self) -> Result<usize, rusqlite::Error> {
        self.conn.execute(T::create_table(), NO_PARAMS)
    }

    fn get_all<T: SQLable>(&self) -> Vec<T> {
        let mut stmnt = self.conn
            .prepare(T::select_all())
            .expect("Could not prepare all statement");

        let iter = stmnt.query_map(NO_PARAMS, |row| T::from_row(row))
            .expect("Could not bind params");

        iter.map(|elem| elem.expect("Could not construct")).collect()
    }

    fn get<T: SQLable>(&self, id: &i64) -> Option<T> {
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

    fn save<T: SQLable>(&mut self, data: &T) -> i64 {
        let mut stmnt = self.conn.prepare(T::insert_one()).expect("Could not prepare save");
        T::bind(data, |bindings| stmnt.insert(bindings)).expect("Could not insert")
    }

    fn find<T: SQLSearchable>(&self, creds: &[<T as Searchable>::Credentials]) -> Vec<T> {
        let (sql, params) = T::build_query(creds, None);

        let mut stmnt = self.conn
            .prepare(&sql)
            .expect("Could not prepare statement");

        let iter = stmnt
            .query_map(&params, |row| T::from_row(row))
            .expect("Could not bind params");

        iter.map(|elem| elem.expect("Could not construct")).collect()
    }
}

use crate::domain::Repository;

// Blanket implementation for all things SQLable
impl<T: SQLable> Repository<T> for Rusqlite {
    type Id = i64;

    fn all(&self) -> Vec<T> {
        self.get_all::<T>()
    }

    fn get(&self, id: &i64) -> Option<T> {
        self.get::<T>(id)
    }

    fn save(&mut self, data: &T) -> Self::Id {
        self.save::<T>(data)
    }
}

use crate::domain::{Searchable, SearchableRepository};
impl<T: SQLSearchable> SearchableRepository<T> for Rusqlite {
    fn find(&self, credentials: &[<T as Searchable>::Credentials]) -> Vec<T> {
        self.find(credentials)
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

    fn get(&self, id: &Self::Id) -> Option<T> {
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

    fn get(&self, id: &Self::Id) -> Option<T> {
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
        let output = repo.get(&id).expect("Could not find what I just put in!");
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


    #[test]
    fn test_searchable() {
        use super::Rusqlite;
        use super::{User, UserSearchTerms};

        let mut repo = Rusqlite::in_memory();
        repo.setup::<User>().expect("Could not setup tables");

        let a = User::new("A");
        let b = User::new("B");
        let c = User::new("C");
        let d = User::new("D");

        repo.save(&a);
        repo.save(&b);
        repo.save(&c);
        repo.save(&d);

        let query_result = repo.find::<User>(&[
            UserSearchTerms::Name("C".to_string()),
        ]);
        assert_eq!(query_result, vec![c]);


    }
}