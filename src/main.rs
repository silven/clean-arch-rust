#![feature(uniform_paths)]

use std::time::{Duration, Instant};

mod domain;
mod data;

use domain::entities::{User, Task};

fn main() {
    use domain::Repository;

    /*
    let mut user_repo = data::TrivialRepository::new();
    let mut task_repo = data::HashRepository::new();
    */
    
    let mut repo = data::Rusqlite::in_memory();
    repo.setup::<User>().expect("Could not setup tables");
    repo.setup::<Task>().expect("Could not setup tables");;

    let mike = User::new("Mike");
    let id = repo.save(&mike);

    let person: User = repo.get(&id).expect("No such person");

    let mut buy_milk = Task::new("Buy Milk").due(Instant::now() + Duration::from_secs(60*24));
    buy_milk.tags = vec!["urgent".into()];
    let task_id = repo.save(&buy_milk);

    let the_task: Task = repo.get(&task_id).expect("No such task");

    println!("{} should {}{:?}", person.name, the_task.desc, the_task.tags);
}
