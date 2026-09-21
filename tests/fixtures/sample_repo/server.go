package main

type ServerHandler struct {
	controller AppController
}

func NewServerHandler(c AppController) *ServerHandler {
	return &ServerHandler{controller: c}
}
