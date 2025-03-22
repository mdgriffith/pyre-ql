# Playground


For testing out Pyre


Getting started:

```sh
bun install
touch db/playground.db
pyre migrate db/playground.db
pyre generate
```


Then, you should be able to run the server:
```sh
bun run dev
```

open http://localhost:3000

And seed some user data via 
```sh
bun run seed
bun run request
```