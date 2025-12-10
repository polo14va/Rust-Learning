// Programa temporal para generar hash
fn main() {
    let password = "test123";
    let hash = bcrypt::hash(password, bcrypt::DEFAULT_COST).unwrap();
    println!("Hash para 'test123': {}", hash);
}
