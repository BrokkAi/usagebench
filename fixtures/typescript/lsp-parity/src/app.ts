import createClient, { ApiClient, formatUser, type User } from "./api";

const client = createClient();
const direct = ApiClient.create("/direct");
const user: User = client.fetchUser("42");
const label = formatUser(user);
const name = user.name;
