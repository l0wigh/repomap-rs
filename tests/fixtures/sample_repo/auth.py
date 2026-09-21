class AuthClient:
    def __init__(self, manager: TokenManager):
        self.manager = manager

    def authenticate(self, token: str) -> bool:
        return self.manager.validate(token)
