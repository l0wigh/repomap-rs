<?php

class ApiController {
    public function handle(UserService $service): void {
        $service->getUser();
    }
}
