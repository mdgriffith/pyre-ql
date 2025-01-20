import * as Query from '../pyre/generated/client/node/query'



async function execute(base_url: string) {
    console.log("Requesting UserNew");
    const result = await Query.UserNew(base_url, { name: "Griff" });
    console.log(result);
}


(async () => {
    try {
        await execute("http://localhost:3000/db");
        process.exit(0); // Add this line
    } catch (err) {
        console.error("Error:", err);
        process.exit(1);
    }
})();