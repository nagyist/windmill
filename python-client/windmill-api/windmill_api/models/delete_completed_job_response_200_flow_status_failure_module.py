from typing import Any, Dict, List, Type, TypeVar, Union, cast

import attr

from ..models.delete_completed_job_response_200_flow_status_failure_module_iterator import (
    DeleteCompletedJobResponse200FlowStatusFailureModuleIterator,
)
from ..models.delete_completed_job_response_200_flow_status_failure_module_type import (
    DeleteCompletedJobResponse200FlowStatusFailureModuleType,
)
from ..types import UNSET, Unset

T = TypeVar("T", bound="DeleteCompletedJobResponse200FlowStatusFailureModule")


@attr.s(auto_attribs=True)
class DeleteCompletedJobResponse200FlowStatusFailureModule:
    """
    Attributes:
        type (DeleteCompletedJobResponse200FlowStatusFailureModuleType):
        job (Union[Unset, str]):
        count (Union[Unset, int]):
        iterator (Union[Unset, DeleteCompletedJobResponse200FlowStatusFailureModuleIterator]):
        forloop_jobs (Union[Unset, List[str]]):
    """

    type: DeleteCompletedJobResponse200FlowStatusFailureModuleType
    job: Union[Unset, str] = UNSET
    count: Union[Unset, int] = UNSET
    iterator: Union[Unset, DeleteCompletedJobResponse200FlowStatusFailureModuleIterator] = UNSET
    forloop_jobs: Union[Unset, List[str]] = UNSET
    additional_properties: Dict[str, Any] = attr.ib(init=False, factory=dict)

    def to_dict(self) -> Dict[str, Any]:
        type = self.type.value

        job = self.job
        count = self.count
        iterator: Union[Unset, Dict[str, Any]] = UNSET
        if not isinstance(self.iterator, Unset):
            iterator = self.iterator.to_dict()

        forloop_jobs: Union[Unset, List[str]] = UNSET
        if not isinstance(self.forloop_jobs, Unset):
            forloop_jobs = self.forloop_jobs

        field_dict: Dict[str, Any] = {}
        field_dict.update(self.additional_properties)
        field_dict.update(
            {
                "type": type,
            }
        )
        if job is not UNSET:
            field_dict["job"] = job
        if count is not UNSET:
            field_dict["count"] = count
        if iterator is not UNSET:
            field_dict["iterator"] = iterator
        if forloop_jobs is not UNSET:
            field_dict["forloop_jobs"] = forloop_jobs

        return field_dict

    @classmethod
    def from_dict(cls: Type[T], src_dict: Dict[str, Any]) -> T:
        d = src_dict.copy()
        type = DeleteCompletedJobResponse200FlowStatusFailureModuleType(d.pop("type"))

        job = d.pop("job", UNSET)

        count = d.pop("count", UNSET)

        _iterator = d.pop("iterator", UNSET)
        iterator: Union[Unset, DeleteCompletedJobResponse200FlowStatusFailureModuleIterator]
        if isinstance(_iterator, Unset):
            iterator = UNSET
        else:
            iterator = DeleteCompletedJobResponse200FlowStatusFailureModuleIterator.from_dict(_iterator)

        forloop_jobs = cast(List[str], d.pop("forloop_jobs", UNSET))

        delete_completed_job_response_200_flow_status_failure_module = cls(
            type=type,
            job=job,
            count=count,
            iterator=iterator,
            forloop_jobs=forloop_jobs,
        )

        delete_completed_job_response_200_flow_status_failure_module.additional_properties = d
        return delete_completed_job_response_200_flow_status_failure_module

    @property
    def additional_keys(self) -> List[str]:
        return list(self.additional_properties.keys())

    def __getitem__(self, key: str) -> Any:
        return self.additional_properties[key]

    def __setitem__(self, key: str, value: Any) -> None:
        self.additional_properties[key] = value

    def __delitem__(self, key: str) -> None:
        del self.additional_properties[key]

    def __contains__(self, key: str) -> bool:
        return key in self.additional_properties
