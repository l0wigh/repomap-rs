pub struct UserService {
    auth: AuthClient,
}

impl UserService {
    pub fn new(auth: AuthClient) -> Self {
        Self { auth }
    }

    pub fn get_user_status(&self) -> bool {
        true
    }
}
