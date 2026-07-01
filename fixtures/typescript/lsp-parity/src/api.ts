export interface User {
  id: string;
  name: string;
}

export class ApiClient {
  static create(baseUrl: string): ApiClient {
    return new ApiClient(baseUrl);
  }

  constructor(private readonly baseUrl: string) {}

  fetchUser(id: string): User {
    return { id, name: this.baseUrl };
  }
}

export function formatUser(user: User): string {
  return user.name;
}

export default function createClient(): ApiClient {
  return ApiClient.create("/api");
}
