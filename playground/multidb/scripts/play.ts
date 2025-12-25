import * as Query from '../pyre/generated/client/node/query'


(async () => {
    const base_url = "http://localhost:3000/db";
    console.log(JSON.stringify(await Query.Games(base_url, {}), null, 2));
    // console.log(JSON.stringify(await Query.UserNew(base_url, { name: "Griff" }), null, 2));
    // console.log("------")
    // console.log(JSON.stringify(await Query.UserInit(base_url, {}), null, 2));

})();