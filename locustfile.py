import os
from locust import HttpUser, task, between


class WebsiteUser(HttpUser):
    # wait_time = between(0,1)

    @task
    def dashboard_with_jwt(self):
        token = os.getenv("LOCUST_JWT")
        headers = {"Authorization": f"Bearer {token}"} if token else {}
        self.client.get("/dashboard", headers=headers)