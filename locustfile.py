from locust import HttpUser, task, between

class WebsiteUser(HttpUser):
    # wait_time = between(0,1)

    @task
    def hello(self):
        self.client.get("/dashboard")