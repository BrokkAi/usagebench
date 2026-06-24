export type ResponsePromise = {
  json(): Promise<unknown>;
};

export type KyInstance = {
  (url: string): ResponsePromise;
  get(url: string): ResponsePromise;
  extend(prefix: string): KyInstance;
};

export class Ky {
  static create(url: string): ResponsePromise {
    return { json: async () => ({ url }) };
  }
}

const createInstance = (prefix = ""): KyInstance => {
  const ky = ((url: string) => Ky.create(`${prefix}${url}`)) as KyInstance;

  ky.get = (url: string) => Ky.create(`${prefix}${url}`);
  ky.extend = (nextPrefix: string) => createInstance(`${prefix}${nextPrefix}`);

  return ky;
};

export const ky = createInstance();

export default ky;
