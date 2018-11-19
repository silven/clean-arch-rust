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
    
    let mut user_repo = data::Rusqlite::<User>::in_memory();
    let mut task_repo = data::Rusqlite::<Task>::in_memory();
    user_repo.setup().expect("Could not setup tables");
    task_repo.setup().expect("Could not setup tables");;

    let mike = User::new("Mike");
    let id = user_repo.save(&mike);

    let person = user_repo.find(&id).expect("No such person");

    let buy_milk = Task::new("Buy Milk").due(Instant::now() + Duration::from_secs(60*24));
    let task_id = task_repo.save(&buy_milk);

    let the_task = task_repo.find(&task_id).expect("No such task");

    println!("{} should {}", person.name, the_task.desc);
}
