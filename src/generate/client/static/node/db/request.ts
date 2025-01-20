import http from 'http';
// import https from 'https';


interface ResponseData {
  statusCode: number;
  data: any;
}

export async function send(url: string, body: Record<string, any>, headers?: Record<string, string>): Promise<ResponseData> {

  const postData = JSON.stringify(body);

  const options = {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'Content-Length': postData.length,
      ...headers
    }
  };

  return new Promise((resolve, reject) => {
    const req = http.request(url, options, async (res: http.IncomingMessage) => {
      try {
        let data = '';

        for await (const chunk of res) {
          data += chunk;
        }

        resolve({
          statusCode: res.statusCode || 500,
          data: JSON.parse(data)
        });
      } catch (error) {
        reject(error);
      }
    });

    req.on('error', (error) => {
      reject(error);
    });

    req.write(postData);
    req.end();
  });
}
