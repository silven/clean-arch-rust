use crate::domain::entities::{User, Task};

use rusqlite::{Connection, NO_PARAMS, types::ToSql};

pub trait SQLable {
    fn select() -> &'static str;
    fn insert() -> &'static str;
    fn create_table() -> &'static str;

    #[inline(always)]
    fn bind<F, T>(data: &Self, consumer: F) -> T where F: FnMut(&'static str, &[&ToSql]) -> T;

    fn from_row<'row, 'stmt>(repo: &Rusqlite, row: &rusqlite::Row<'row, 'stmt>) -> Self;
}

pub struct QueryValue<'query>(&'static str, &'query ToSql);

pub trait SQLSearchable : Searchable + SQLable {
    fn build_query(creds: &[<Self as Searchable>::Credentials]) -> Vec<QueryValue>;
}

// Some kind of generalization so I can extract the things that differ.
// The major drawback I found with this, is the problems related to the relation
// between different objects. A User has Tasks, but I don't get them like this
// and a LEFT JOIN doesn't really help, because then we need to post-process the data
impl SQLable for User {
    fn select() -> &'static str { "SELECT * FROM users" }
    fn insert() -> &'static str { "INSERT INTO users (name) VALUES (?)" }
    fn create_table() -> &'static str {
        "CREATE TABLE users (
            id         INTEGER PRIMARY KEY,
            name       TEXT NOT NULL
        )"
    }

    fn bind<F, T>(data: &Self, mut consumer: F) -> T where F: FnMut(&'static str, &[&ToSql]) -> T {
        let bindings: [&ToSql; 1] = [&data.name];
        let my_id = consumer(Self::insert(), &bindings);

        // TODO: optimize with bulk insert
        for task in data.tasks() {
            Task::bind(task, &mut consumer);
        }

        my_id
    }

    fn from_row<'row, 'stmt>(repo: &Rusqlite, row: &rusqlite::Row<'row, 'stmt>) -> Self {
        let id: <Rusqlite as Repository<User>>::Id = row.get(0);
        let name: String = row.get(1);
        let tasks = repo.query(&[QueryValue("id", &id)], None);
        User::with_tasks(name, tasks)
    }
}

impl SQLSearchable for User {
    fn build_query(creds: &[<Self as Searchable>::Credentials]) -> Vec<QueryValue> {
        let mut result = Vec::with_capacity(creds.len());
        for pred in creds {
            match pred {
                UserSearchTerms::Name(ref name) => {
                    result.push(QueryValue("name", name));
                }
            }
        }
        result
    }
}

impl SQLable for Task {
    fn select() -> &'static str { "SELECT * from tasks" }
    fn insert() -> &'static str { "INSERT INTO tasks (desc, done, tags) VALUES (?, ?, ?)" }
    fn create_table() -> &'static str {
        "CREATE TABLE tasks (
            id         INTEGER PRIMARY KEY,
            desc       TEXT NOT NULL,
            done       BOOL NOT NULL,
            tags       TEXT
        )"
    }

    fn bind<F, T>(data: &Self, mut consumer: F) -> T where F: FnMut(&'static str, &[&ToSql]) -> T {
        let joined = data.tags.join(",");
        let tags: &ToSql = if joined.len() > 0 { &joined } else { &rusqlite::types::Null };
        let bindings: [&ToSql; 3] = [&data.desc, &data.done, &tags];
        let id = consumer(Self::insert(), &bindings);
        id
    }

    fn from_row<'row, 'stmt>(_repo: &Rusqlite, row: &rusqlite::Row<'row, 'stmt>) -> Self {
        let desc: String = row.get("desc");
        let done: bool = row.get("done");
        let tags: Option<String> = row.get("tags");

        let tag_vec = tags.map_or(vec![], |s| s.split(",").map(Into::into).collect());
        Task {
            desc: desc,
            done: done,
            tags: tag_vec,
            due: None, // No support here yet, lawl
        }
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
        self.query(&[], None)
    }

    fn get<T: SQLable>(&self, id: &i64) -> Option<T> {
        let result = self.query(&[QueryValue("id", id)], Some(1));
        result.into_iter().next()
    }

    fn save<T: SQLable>(&mut self, data: &T) -> i64 {
        T::bind(data, |sql, bindings| {
            let mut stmnt = self.conn.prepare(sql).expect("Could not prepare save");
            stmnt.insert(bindings).expect("Could not insert")
        })
    }

    fn query<T: SQLable>(&self, parameters: &[QueryValue], limit: Option<u32>) -> Vec<T> {
        let mut sql = T::select().to_string();

        if parameters.len() > 0 {
            sql += " WHERE";
        }

        let mut params: Vec<&ToSql> = Vec::with_capacity(parameters.len());
        let mut iter = parameters.iter().peekable();

        while let Some(QueryValue(field, value)) = iter.next() {
            sql += &format!(" {} = (?)", field);
            params.push(value);

            if iter.peek().is_some() {
                sql += " AND";
            }
        }

        if let Some(limit) = limit {
            sql += &format!(" LIMIT {}", limit);
        }

        let mut stmnt = self.conn
            .prepare(&sql)
            .expect("Could not prepare statement");

        let iter = stmnt
            .query_map(&params, |row| T::from_row(&self, row))
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
    fn find(&self, credentials: &[<T as Searchable>::Credentials], limit: Option<u32>) -> Vec<T> {
        let query_data = T::build_query(credentials);
        self.query(&query_data, limit)
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
        use super::{User, Task, UserSearchTerms};
        use crate::domain::SearchableRepository;

        let mut repo = Rusqlite::in_memory();
        repo.setup::<User>().expect("Could not setup tables");
        repo.setup::<Task>().expect("Could not setup tables");

        let dummy_task = Task::new("Buy milk");

        let a = User::with_tasks("A", vec![dummy_task.clone()]);
        let b = User::with_tasks("B", vec![dummy_task.clone()]);
        let c = User::with_tasks("C", vec![dummy_task.clone()]);
        let d = User::with_tasks("D", vec![dummy_task.clone()]);

        repo.save(&a);
        repo.save(&b);
        repo.save(&c);
        repo.save(&d);

        let query_result: Vec<User> = repo.find(
            &[UserSearchTerms::Name("C".to_string())],
            Some(1));
        assert_eq!(query_result, vec![c]);


    }
}