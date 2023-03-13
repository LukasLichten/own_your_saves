## Build Instructions
### Requirements:
Rust 1.67.1 or newer  
```
cargo install trunk wasm-bindgen-cli
rustup target add wasm32-unknown-unknown
```

### Debug Run:
```
# Frontend
cd frontend
trunk serve
# Backend
cargo run
```
Frontend is accessed on localhost:8080  
API can be then accessed on localhost:80/api/, or localhost:8080/api/  
  
Backend includes code to serve the frontend, and will show errors when run locally.  
These are harmless, to make them go away you need to `ln -s ./frontend/dist ./dist`, which will then serve a static build on port 80.  
But for Development using `trunk serve` and then loading the page on port 8080 is preferred as allows hotreloading.  
  
### Production Build
```
docker build .
```