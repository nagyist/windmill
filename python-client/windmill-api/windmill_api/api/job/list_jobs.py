import datetime
from typing import Any, Dict, List, Optional, Union

import httpx

from ...client import Client
from ...models.list_jobs_response_200_item import ListJobsResponse200Item
from ...types import UNSET, Response, Unset


def _get_kwargs(
    workspace: str,
    *,
    client: Client,
    created_by: Union[Unset, None, str] = UNSET,
    parent_job: Union[Unset, None, str] = UNSET,
    script_path_exact: Union[Unset, None, str] = UNSET,
    script_path_start: Union[Unset, None, str] = UNSET,
    script_hash: Union[Unset, None, str] = UNSET,
    created_before: Union[Unset, None, datetime.datetime] = UNSET,
    created_after: Union[Unset, None, datetime.datetime] = UNSET,
    job_kinds: Union[Unset, None, str] = UNSET,
    is_skipped: Union[Unset, None, bool] = UNSET,
    is_flow_step: Union[Unset, None, bool] = UNSET,
    success: Union[Unset, None, bool] = UNSET,
) -> Dict[str, Any]:
    url = "{}/w/{workspace}/jobs/list".format(client.base_url, workspace=workspace)

    headers: Dict[str, str] = client.get_headers()
    cookies: Dict[str, Any] = client.get_cookies()

    params: Dict[str, Any] = {}
    params["created_by"] = created_by

    params["parent_job"] = parent_job

    params["script_path_exact"] = script_path_exact

    params["script_path_start"] = script_path_start

    params["script_hash"] = script_hash

    json_created_before: Union[Unset, None, str] = UNSET
    if not isinstance(created_before, Unset):
        json_created_before = created_before.isoformat() if created_before else None

    params["created_before"] = json_created_before

    json_created_after: Union[Unset, None, str] = UNSET
    if not isinstance(created_after, Unset):
        json_created_after = created_after.isoformat() if created_after else None

    params["created_after"] = json_created_after

    params["job_kinds"] = job_kinds

    params["is_skipped"] = is_skipped

    params["is_flow_step"] = is_flow_step

    params["success"] = success

    params = {k: v for k, v in params.items() if v is not UNSET and v is not None}

    return {
        "method": "get",
        "url": url,
        "headers": headers,
        "cookies": cookies,
        "timeout": client.get_timeout(),
        "params": params,
    }


def _parse_response(*, response: httpx.Response) -> Optional[List[ListJobsResponse200Item]]:
    if response.status_code == 200:
        response_200 = []
        _response_200 = response.json()
        for response_200_item_data in _response_200:
            response_200_item = ListJobsResponse200Item.from_dict(response_200_item_data)

            response_200.append(response_200_item)

        return response_200
    return None


def _build_response(*, response: httpx.Response) -> Response[List[ListJobsResponse200Item]]:
    return Response(
        status_code=response.status_code,
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(response=response),
    )


def sync_detailed(
    workspace: str,
    *,
    client: Client,
    created_by: Union[Unset, None, str] = UNSET,
    parent_job: Union[Unset, None, str] = UNSET,
    script_path_exact: Union[Unset, None, str] = UNSET,
    script_path_start: Union[Unset, None, str] = UNSET,
    script_hash: Union[Unset, None, str] = UNSET,
    created_before: Union[Unset, None, datetime.datetime] = UNSET,
    created_after: Union[Unset, None, datetime.datetime] = UNSET,
    job_kinds: Union[Unset, None, str] = UNSET,
    is_skipped: Union[Unset, None, bool] = UNSET,
    is_flow_step: Union[Unset, None, bool] = UNSET,
    success: Union[Unset, None, bool] = UNSET,
) -> Response[List[ListJobsResponse200Item]]:
    """list all available jobs

    Args:
        workspace (str):
        created_by (Union[Unset, None, str]):
        parent_job (Union[Unset, None, str]):
        script_path_exact (Union[Unset, None, str]):
        script_path_start (Union[Unset, None, str]):
        script_hash (Union[Unset, None, str]):
        created_before (Union[Unset, None, datetime.datetime]):
        created_after (Union[Unset, None, datetime.datetime]):
        job_kinds (Union[Unset, None, str]):
        is_skipped (Union[Unset, None, bool]):
        is_flow_step (Union[Unset, None, bool]):
        success (Union[Unset, None, bool]):

    Returns:
        Response[List[ListJobsResponse200Item]]
    """

    kwargs = _get_kwargs(
        workspace=workspace,
        client=client,
        created_by=created_by,
        parent_job=parent_job,
        script_path_exact=script_path_exact,
        script_path_start=script_path_start,
        script_hash=script_hash,
        created_before=created_before,
        created_after=created_after,
        job_kinds=job_kinds,
        is_skipped=is_skipped,
        is_flow_step=is_flow_step,
        success=success,
    )

    response = httpx.request(
        verify=client.verify_ssl,
        **kwargs,
    )

    return _build_response(response=response)


def sync(
    workspace: str,
    *,
    client: Client,
    created_by: Union[Unset, None, str] = UNSET,
    parent_job: Union[Unset, None, str] = UNSET,
    script_path_exact: Union[Unset, None, str] = UNSET,
    script_path_start: Union[Unset, None, str] = UNSET,
    script_hash: Union[Unset, None, str] = UNSET,
    created_before: Union[Unset, None, datetime.datetime] = UNSET,
    created_after: Union[Unset, None, datetime.datetime] = UNSET,
    job_kinds: Union[Unset, None, str] = UNSET,
    is_skipped: Union[Unset, None, bool] = UNSET,
    is_flow_step: Union[Unset, None, bool] = UNSET,
    success: Union[Unset, None, bool] = UNSET,
) -> Optional[List[ListJobsResponse200Item]]:
    """list all available jobs

    Args:
        workspace (str):
        created_by (Union[Unset, None, str]):
        parent_job (Union[Unset, None, str]):
        script_path_exact (Union[Unset, None, str]):
        script_path_start (Union[Unset, None, str]):
        script_hash (Union[Unset, None, str]):
        created_before (Union[Unset, None, datetime.datetime]):
        created_after (Union[Unset, None, datetime.datetime]):
        job_kinds (Union[Unset, None, str]):
        is_skipped (Union[Unset, None, bool]):
        is_flow_step (Union[Unset, None, bool]):
        success (Union[Unset, None, bool]):

    Returns:
        Response[List[ListJobsResponse200Item]]
    """

    return sync_detailed(
        workspace=workspace,
        client=client,
        created_by=created_by,
        parent_job=parent_job,
        script_path_exact=script_path_exact,
        script_path_start=script_path_start,
        script_hash=script_hash,
        created_before=created_before,
        created_after=created_after,
        job_kinds=job_kinds,
        is_skipped=is_skipped,
        is_flow_step=is_flow_step,
        success=success,
    ).parsed


async def asyncio_detailed(
    workspace: str,
    *,
    client: Client,
    created_by: Union[Unset, None, str] = UNSET,
    parent_job: Union[Unset, None, str] = UNSET,
    script_path_exact: Union[Unset, None, str] = UNSET,
    script_path_start: Union[Unset, None, str] = UNSET,
    script_hash: Union[Unset, None, str] = UNSET,
    created_before: Union[Unset, None, datetime.datetime] = UNSET,
    created_after: Union[Unset, None, datetime.datetime] = UNSET,
    job_kinds: Union[Unset, None, str] = UNSET,
    is_skipped: Union[Unset, None, bool] = UNSET,
    is_flow_step: Union[Unset, None, bool] = UNSET,
    success: Union[Unset, None, bool] = UNSET,
) -> Response[List[ListJobsResponse200Item]]:
    """list all available jobs

    Args:
        workspace (str):
        created_by (Union[Unset, None, str]):
        parent_job (Union[Unset, None, str]):
        script_path_exact (Union[Unset, None, str]):
        script_path_start (Union[Unset, None, str]):
        script_hash (Union[Unset, None, str]):
        created_before (Union[Unset, None, datetime.datetime]):
        created_after (Union[Unset, None, datetime.datetime]):
        job_kinds (Union[Unset, None, str]):
        is_skipped (Union[Unset, None, bool]):
        is_flow_step (Union[Unset, None, bool]):
        success (Union[Unset, None, bool]):

    Returns:
        Response[List[ListJobsResponse200Item]]
    """

    kwargs = _get_kwargs(
        workspace=workspace,
        client=client,
        created_by=created_by,
        parent_job=parent_job,
        script_path_exact=script_path_exact,
        script_path_start=script_path_start,
        script_hash=script_hash,
        created_before=created_before,
        created_after=created_after,
        job_kinds=job_kinds,
        is_skipped=is_skipped,
        is_flow_step=is_flow_step,
        success=success,
    )

    async with httpx.AsyncClient(verify=client.verify_ssl) as _client:
        response = await _client.request(**kwargs)

    return _build_response(response=response)


async def asyncio(
    workspace: str,
    *,
    client: Client,
    created_by: Union[Unset, None, str] = UNSET,
    parent_job: Union[Unset, None, str] = UNSET,
    script_path_exact: Union[Unset, None, str] = UNSET,
    script_path_start: Union[Unset, None, str] = UNSET,
    script_hash: Union[Unset, None, str] = UNSET,
    created_before: Union[Unset, None, datetime.datetime] = UNSET,
    created_after: Union[Unset, None, datetime.datetime] = UNSET,
    job_kinds: Union[Unset, None, str] = UNSET,
    is_skipped: Union[Unset, None, bool] = UNSET,
    is_flow_step: Union[Unset, None, bool] = UNSET,
    success: Union[Unset, None, bool] = UNSET,
) -> Optional[List[ListJobsResponse200Item]]:
    """list all available jobs

    Args:
        workspace (str):
        created_by (Union[Unset, None, str]):
        parent_job (Union[Unset, None, str]):
        script_path_exact (Union[Unset, None, str]):
        script_path_start (Union[Unset, None, str]):
        script_hash (Union[Unset, None, str]):
        created_before (Union[Unset, None, datetime.datetime]):
        created_after (Union[Unset, None, datetime.datetime]):
        job_kinds (Union[Unset, None, str]):
        is_skipped (Union[Unset, None, bool]):
        is_flow_step (Union[Unset, None, bool]):
        success (Union[Unset, None, bool]):

    Returns:
        Response[List[ListJobsResponse200Item]]
    """

    return (
        await asyncio_detailed(
            workspace=workspace,
            client=client,
            created_by=created_by,
            parent_job=parent_job,
            script_path_exact=script_path_exact,
            script_path_start=script_path_start,
            script_hash=script_hash,
            created_before=created_before,
            created_after=created_after,
            job_kinds=job_kinds,
            is_skipped=is_skipped,
            is_flow_step=is_flow_step,
            success=success,
        )
    ).parsed
