export interface BlogPost {
  slug: string;
  title: string;
  author: string;
  date: string;
  content: string;
}

export interface BlogData {
  posts: BlogPost[];
}
