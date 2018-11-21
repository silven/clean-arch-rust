pub mod entities;


pub trait Searchable {
    type Credentials;
}

pub trait Repository<T> {
    // Since different databases use different Ids, 
    // I think I should be able to parameterize over it.
    type Id;

    fn all(&self) -> Vec<T>;
    fn get(&self, id: &Self::Id) -> Option<T>;
    fn save(&mut self, data: &T) -> Self::Id;
}

pub trait SearchableRepository<T: Searchable> : Repository<T> {
    fn find(&self, id: &[T::Credentials]) -> Vec<T>;
}

// The structure is very ad-hoc
pub mod usecases {
    use super::entities::{User, Task};

    pub fn find_all_done(user: &User) -> Vec<Task> {
        user.tasks().iter().filter(|t| t.is_done()).cloned().collect()
    }

    // Unsure if this belongs one this level or not
    use super::Repository;
    pub fn find_all_done_via_id<R: Repository<User>>(repo: &R, id: &R::Id) -> Vec<Task> {
        let user = repo.get(&id).expect("No such user!");
        find_all_done(&user)
    }

    #[cfg(test)] 
    mod test {
        use super::{User, Task};
        
        #[test]
        fn test_get_all_done() {
            let mut one_done = Task::new("One");
            one_done.finish();
            let mut two_done = Task::new("Two");
            two_done.finish();

            let not_done = Task::new("Tre");

            let mut user: User = User::new("Someone");

            user.add_task(one_done.clone());
            user.add_task(not_done.clone());
            user.add_task(two_done.clone());

            let found_done = super::find_all_done(&user);

            assert_eq!(found_done, vec![one_done, two_done]);   
        }

    }
} 