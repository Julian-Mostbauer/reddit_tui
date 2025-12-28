pub struct Post {
  author: String,
  num_comments: u32,
  media_url: Option<String>,
  selftext: Option<String>,
  perma_link: String,
  subreddit: String,
  title: String,
  score: u32,
  upvote_ratio: f32,
  creation_time: i64
}

/*
structure of the json for a post inside a post listing, for example the reddit front page
{
kind: str,
data: {
  -- some unimportant fields
  children: [
      {
        kind: str, -- allways t3 for posts
        data: {
            -- here is the actual data of the post. There are many fields, most are not used by my model or under a different name
        }
      }
    ]
  }
}
*/

pub struct Comment {
  body: String,
  replies: Vec<Comment>,
  score: u32,
  author: String,
  permalink: String
}

/*
structure of the json for a post to find its comments
[
-- the first object is unimportant
  {
    kind: str,
    data: obj
  },
-- the second object contains the usefull data
  {
    kind: str,
    data: {
      -- a bunch of unimportant misc. fields
      -- the children array contains the comments.
      children: [
        {
        kind: str, -- for comments allways t1
        data: {
          -- here is the actual data of the comments. There are many fields, most are not used by my model or under a different name
          }
        }
      ]
    }
  },
  
]
 */