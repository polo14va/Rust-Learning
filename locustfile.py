import os
from locust import HttpUser, task, between


class WebsiteUser(HttpUser):
    # wait_time = between(0,1)

    @task
    def dashboard_with_jwt(self):
        token = os.getenv("eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJhZG1pbjEiLCJleHAiOjE3NjU0MzUxMzl9.XMs6DqNU1Tk3STw3apoxuMgYDDJ2TC63-ipY57LuSLw")
        headers = {"Authorization": f"Bearer {token}"} if token else {}
        self.client.get("/dashboard", headers=headers)