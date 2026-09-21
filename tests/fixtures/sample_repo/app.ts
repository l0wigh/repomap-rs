export class AppController {
    private service: UserService;

    constructor(service: UserService) {
        this.service = service;
    }

    public run(): void {
        this.service.get_user_status();
    }
}
