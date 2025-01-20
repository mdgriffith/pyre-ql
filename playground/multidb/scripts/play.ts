import * as Query from '../pyre/generated/client/node/query'


(async () => {
    const base_url = "http://localhost:3000/db";
    console.log(await Query.UserNew(base_url, { name: "Griff" }));
    console.log(await Query.UserInit(base_url, {}));

})();