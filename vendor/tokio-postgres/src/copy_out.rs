use crate::client::{InnerClient, Responses};
use crate::codec::FrontendMessage;
use crate::connection::RequestMessages;
use crate::{Error, Statement, query, slice_iter};
use bytes::Bytes;
use futures_util::Stream;
use log::debug;
use pin_project_lite::pin_project;
use postgres_protocol::message::backend::Message;
use std::pin::Pin;
use std::task::{Context, Poll, ready};

pub async fn copy_out(client: &InnerClient, statement: Statement) -> Result<CopyOutStream, Error> {
    debug!("executing copy out statement {}", statement.name());

    let buf = query::encode(client, &statement, slice_iter(&[]))?;
    let responses = start(client, buf).await?;
    Ok(CopyOutStream { responses })
}

async fn start(client: &InnerClient, buf: Bytes) -> Result<Responses, Error> {
    let mut responses = client.send(RequestMessages::Single(FrontendMessage::Raw(buf)))?;

    match responses.next().await? {
        Message::BindComplete => {}
        _ => return Err(Error::unexpected_message()),
    }

    match responses.next().await? {
        Message::CopyOutResponse(_) => {}
        _ => return Err(Error::unexpected_message()),
    }

    Ok(responses)
}

pin_project! {
    /// A stream of `COPY ... TO STDOUT` query data.
    #[project(!Unpin)]
    pub struct CopyOutStream {
        responses: Responses,
    }
}

impl Stream for CopyOutStream {
    type Item = Result<Bytes, Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();

        match ready!(this.responses.poll_next(cx)?) {
            Message::CopyData(body) => Poll::Ready(Some(Ok(body.into_bytes()))),
            Message::CopyDone => Poll::Ready(None),
            _ => Poll::Ready(Some(Err(Error::unexpected_message()))),
        }
    }
}

impl CopyOutStream {
    /// Yields COPY data and waits for this request's command completion and
    /// ready-for-query response before ending. Errors after the last data
    /// message are returned as stream items.
    pub fn with_completion(self) -> impl Stream<Item = Result<Bytes, Error>> {
        futures_util::stream::try_unfold(self.responses, |mut responses| async move {
            match responses.next().await? {
                Message::CopyData(body) => return Ok(Some((body.into_bytes(), responses))),
                Message::CopyDone => {}
                _ => return Err(Error::unexpected_message()),
            }

            let Message::CommandComplete(body) = responses.next().await? else {
                return Err(Error::unexpected_message());
            };
            let tag = body.tag().map_err(Error::parse)?;
            if !tag.strip_prefix("COPY ").is_some_and(|rows| {
                !rows.is_empty() && rows.bytes().all(|byte| byte.is_ascii_digit())
            }) {
                return Err(Error::unexpected_message());
            }

            match responses.next().await? {
                Message::ReadyForQuery(_) => Ok(None),
                _ => Err(Error::unexpected_message()),
            }
        })
    }
}
